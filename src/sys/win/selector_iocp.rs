use {EventEntry, EventFlags, FLAG_READ, FLAG_WRITE, FLAG_ACCEPT, FLAG_ENDED, EventBuffer, EventLoop, RetValue};
use std::collections::HashMap;
use std::mem;
use psocket::SOCKET;
use std::cell::UnsafeCell;
use std::ptr;
use std::io::{self, ErrorKind};
use std::time::Duration;
use sys::win::iocp::{CompletionPort, CompletionStatus};
use sys::win::{FromRawArc, Overlapped};
use super::{TcpSocketExt, AcceptAddrsBuf};
use psocket::{TcpSocket, SocketAddr};
use winapi;
use winapi::*;
use kernel32;
use ws2_32::*;
use std::io::prelude::*;


macro_rules! overlapped2arc {
    ($e:expr, $t:ty, $($field:ident).+) => ({
        unsafe {
            let offset = offset_of!($t, $($field).+);
            debug_assert!(offset < mem::size_of::<$t>());
            FromRawArc::from_raw(($e as usize - offset) as *mut $t)
        }
    })
}

macro_rules! offset_of {
    ($t:ty, $($field:ident).+) => (
        &(*(0 as *const $t)).$($field).+ as *const _ as usize
    )
}

struct StreamIo {
    read: Overlapped, // also used for connect
    write: Overlapped,
}

struct ListenerIo {
    accept_buf: AcceptAddrsBuf,
    accept: Overlapped,
}

pub struct Event {
    pub buffer: EventBuffer,
    pub entry: EventEntry,
    pub read: CbOverlapped,
    pub write: CbOverlapped,
    pub accept_buf: AcceptAddrsBuf,
    pub accept_socket: TcpSocket,
    pub is_end: bool,
}

impl Event {
    pub fn is_accept(&self) -> bool {
        self.entry.ev_events.contains(FLAG_ACCEPT)
    }

    pub fn get_event_socket(&self) -> SOCKET {
        self.buffer.socket.as_raw_socket()
    }

    // pub fn event_cb(&self, ev: &mut EventLoop) -> RetValue {
    //     self.entry.EventCb(ev, &mut self.buffer)
    // }
}

#[derive(Clone)]
pub struct EventImpl {
    pub inner: FromRawArc<Event>,
}

impl EventImpl {
    pub fn new(event: Event) -> EventImpl {
        EventImpl {
            inner: FromRawArc::new(event)
        }
    }
}

#[derive(Debug)]
pub struct Events {
    /// Raw I/O event completions are filled in here by the call to `get_many`
    /// on the completion port above. These are then processed to run callbacks
    /// which figure out what to do after the event is done.
    statuses: Box<[CompletionStatus]>,
}

#[repr(C)]
pub struct CbOverlapped {
    inner: UnsafeCell<Overlapped>,
    callback: fn(&mut EventLoop, &OVERLAPPED_ENTRY) -> RetValue,
}


impl CbOverlapped {
    /// Creates a new `Overlapped` which will invoke the provided `cb` callback
    /// whenever it's triggered.
    ///
    /// The returned `Overlapped` must be used as the `OVERLAPPED` passed to all
    /// I/O operations that are registered with mio's event loop. When the I/O
    /// operation associated with an `OVERLAPPED` pointer completes the event
    /// loop will invoke the function pointer provided by `cb`.
    pub fn new(cb: fn(&mut EventLoop, &OVERLAPPED_ENTRY) -> RetValue) -> CbOverlapped {
        CbOverlapped {
            inner: UnsafeCell::new(Overlapped::zero()),
            callback: cb,
        }
    }

    /// Get the underlying `Overlapped` instance as a raw pointer.
    ///
    /// This can be useful when only a shared borrow is held and the overlapped
    /// pointer needs to be passed down to winapi.
    pub fn as_mut_ptr(&self) -> *mut OVERLAPPED {
        unsafe {
            (*self.inner.get()).raw()
        }
    }
}


impl Events {
    pub fn with_capacity(cap: usize) -> Events {
        // Note that it's possible for the output `events` to grow beyond the
        // capacity as it can also include deferred events, but that's certainly
        // not the end of the world!
        Events {
            statuses: vec![CompletionStatus::zero(); cap].into_boxed_slice(),
            // events: Vec::with_capacity(cap),
        }
    }

    // pub fn is_empty(&self) -> bool {
    //     self.events.is_empty()
    // }

    // pub fn len(&self) -> usize {
    //     self.events.len()
    // }

    // pub fn capacity(&self) -> usize {
    //     self.events.capacity()
    // }

    // pub fn get(&self, idx: usize) -> Option<Event> {
    //     self.events.get(idx).map(|e| *e)
    // }

    // pub fn push_event(&mut self, event: Event) {
    //     self.events.push(event);
    // }
}

unsafe fn cancel(socket: &TcpSocket,
                 overlapped: &CbOverlapped) -> io::Result<()> {
    let handle = socket.as_raw_socket() as winapi::HANDLE;
    let ret = kernel32::CancelIoEx(handle, overlapped.as_mut_ptr());
    if ret == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn accept_done(event: &mut EventLoop, status: &OVERLAPPED_ENTRY) -> RetValue {
    println!("accept_done 1111111111111111");
    let status = CompletionStatus::from_entry(status);
    let mut event = overlapped2arc!(status.overlapped(), Event, read);
    let socket = mem::replace(&mut event.accept_socket, TcpSocket::new_invalid().unwrap());
    println!("socket new is = {:?}", socket);

    // trace!("finished an accept");
    // let result = me2.inner.socket.accept_complete(&socket).and_then(|()| {
    //     me.accept_buf.parse(&me2.inner.socket)
    // }).and_then(|buf| {
    //     buf.remote().ok_or_else(|| {
    //         io::Error::new(ErrorKind::Other, "could not obtain remote address")
    //     })
    // });
    // me.accept = match result {
    //     Ok(remote_addr) => State::Ready((socket, remote_addr)),
    //     Err(e) => State::Error(e),
    // };
    // me2.add_readiness(&mut me, Ready::readable());
    RetValue::OK
}


fn read_done(event_loop: &mut EventLoop, status: &OVERLAPPED_ENTRY) -> RetValue {
    println!("read_done 1111111111111111");
    let status = CompletionStatus::from_entry(status);
    let mut event = overlapped2arc!(status.overlapped(), Event, read);
    if event.is_accept() {
        let mut socket = mem::replace(&mut event.accept_socket, TcpSocket::new_invalid().unwrap());
        let result = event.buffer.socket.accept_complete(&socket).and_then(|()| {
            event.accept_buf.parse(&event.buffer.socket)
        }).and_then(|buf| {
            buf.remote().ok_or_else(|| {
                io::Error::new(ErrorKind::Other, "could not obtain remote address")
            })
        });
        let ret = match result {
            Ok(remote_addr) => { 
                socket.set_peer_addr(remote_addr);
                println!("socket new is = {:?}", socket);
                event.entry.accept_cb(event_loop, Ok(socket))
            }
            Err(e) => {
                event.entry.accept_cb(event_loop, Err(e))
            },
        };

        match ret {
            RetValue::OVER => {
                event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
            }
            _ => {
                if let Err(_) = event_loop.selector.post_accept_event(event.get_event_socket()) {
                    event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
                }
            },
        }
        return RetValue::OK;
    } else {
        let mut bytes_transferred = status.bytes_transferred() as usize;
        println!("bytes_transferred = {:?}", bytes_transferred);

        if status.flag().contains(FLAG_ENDED) {
            Selector::_unregister_socket(event_loop, event.buffer.as_raw_socket(), EventFlags::all());
            return RetValue::OK;
        }

        if bytes_transferred == 0 {
            Selector::unregister_socket(event_loop, event.buffer.as_raw_socket(), EventFlags::all());
            return RetValue::OK;
        }
        let mut event_clone = event.clone();
        println!("len = {:?}", event_clone.buffer.read_cache.len());
        println!("read = {:?}", event.buffer.read);
        println!("socket = {:?}", event.buffer.as_raw_socket());
        if bytes_transferred > 0 {
            println!("???????");
            let _ = event.buffer.read.write(&event_clone.buffer.read_cache[..bytes_transferred]);
            println!("!!!!!!!");
        }
        
        println!("2222222222222222");

        let res = event_loop.selector._do_read_all(event.get_event_socket());
        if event.buffer.has_read_buffer() {
            match event.entry.EventCb(event_loop, &mut event_clone.buffer) {
                RetValue::OVER => {
                    event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
                    return RetValue::OK;
                },
                _ => (),
            }
        }
        println!("11111111111res = {:?}", res);
        match res {
            Err(_) => {
                event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
            }
            Ok(n) => {
                bytes_transferred = bytes_transferred + n;
            }
        }

        // if bytes_transferred == 0 {
        //     println!("all receive is null");
        //     // 投递完成事件却没有任何数据返回, 则认为该socket已被关闭
        //     event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
        //     return RetValue::OK;
        // }

        // println!("read!!!!!!!!!!!!!!!");
        // println!("status.flag() {:?}", status.flag());
        // let mut event_clone = event.clone();
        // // 通知已读数据事件回来
        // if status.flag().contains(FLAG_ENDED) {
        //     match event.entry.EventCb(event_loop, &mut event_clone.buffer) {
        //         RetValue::OVER => {
        //             event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
        //         },
        //         _ => (),
        //     }
        // } else {
        //     event.buffer.is_in_read = false;
        //     let res = event_loop.selector._do_read_all(event.get_event_socket());
        //     if event.buffer.has_read_buffer() {
        //         match event.entry.EventCb(event_loop, &mut event_clone.buffer) {
        //             RetValue::OVER => {
        //                 event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
        //                 return RetValue::OK;
        //             },
        //             _ => (),
        //         }
        //     }
        //     if let Err(_) = res {
        //         event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
        //     }
        // }
    }
    RetValue::OK
}

fn write_done(event_loop: &mut EventLoop, status: &OVERLAPPED_ENTRY) -> RetValue {
    println!("write_done 1111111111111111");
    let status = CompletionStatus::from_entry(status);
    let mut event = overlapped2arc!(status.overlapped(), Event, write);
    event.buffer.is_in_write = false;

    event_loop.selector._do_write_all(&event.buffer.as_raw_socket(), None);

    RetValue::OK
}

impl Event {
    pub fn new(buffer: EventBuffer, entry: EventEntry) -> Event {
        Event {
            buffer: buffer,
            entry: entry,
            read: CbOverlapped::new(read_done),
            write: CbOverlapped::new(write_done),
            accept_socket: TcpSocket::new_invalid().unwrap(),
            accept_buf: AcceptAddrsBuf::new(),
            is_end: false,
        }
    }
}

pub struct Selector {
    port: CompletionPort,
    events: Events,
    event_maps: HashMap<SOCKET, EventImpl>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        Ok(Selector {
            port: CompletionPort::new(1)?,
            events: Events::with_capacity(1024),
            event_maps: HashMap::new(),
        })
    }

    
    pub fn do_select1(&mut self, event: &mut EventLoop, timeout: u32) -> io::Result<u32> {
        Ok(1)
    }

    pub fn do_select(event: &mut EventLoop, timeout: u32) -> io::Result<u32> {
        let n = match event.selector.port.get_many(&mut event.selector.events.statuses, Some(Duration::from_millis(timeout as u64))) {
            Ok(statuses) => statuses.len(),
            Err(ref e) if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) => 0,
            Err(e) => return Err(e),
        };

        if n > 0 {
            println!("do_select n = {:?}", n);
        }


        let statuses = event.selector.events.statuses[..n].to_vec();
        let mut ret = false;

        // for status in &statuses {
        //     println!("aaaaaaaaaaaa11111 flags = {:?}", status.flag());
        //     println!("aaaaaaaaaaaa11111 byte = {:?}", status.bytes_transferred());
        //     if status.overlapped() as usize == 0 {
        //         println!("zero!!!!!!!!!");
        //         continue;
        //     }

        //     let mut event = overlapped2arc!(status.overlapped(), Event, read);
        //     println!("socket flags = {:?}", event.buffer.as_raw_socket());
        //     mem::forget(event);
        // }

        for status in statuses {
            println!("aaaaaaaaaaaa11111 flags = {:?}", status.flag());
            // This should only ever happen from the awakener, and we should
            // only ever have one awakener right now, so assert as such.
            if status.overlapped() as usize == 0 {
                // assert_eq!(status.token(), usize::from(awakener));
                ret = true;
                continue;
            }

            println!("!!!!flags = {:?}", status.flag());



            let callback = unsafe {
                (*(status.overlapped() as *mut CbOverlapped)).callback
            };

            println!("select; -> got overlapped");
            callback(event, status.entry());
        }

        Ok(0)
    }

    pub fn select(&mut self, evts: &mut Vec<EventEntry>, timeout: u32) -> io::Result<u32> {
        let n = match self.port.get_many(&mut self.events.statuses, Some(Duration::from_millis(timeout as u64))) {
            Ok(statuses) => statuses.len(),
            Err(ref e) if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) => 0,
            Err(e) => return Err(e),
        };

        let mut ret = false;
        for status in self.events.statuses[..n].iter() {
            println!("aaaaaaaaaaaa11111");
            // This should only ever happen from the awakener, and we should
            // only ever have one awakener right now, so assert as such.
            if status.overlapped() as usize == 0 {
                // assert_eq!(status.token(), usize::from(awakener));
                ret = true;
                continue;
            }

            let callback = unsafe {
                (*(status.overlapped() as *mut CbOverlapped)).callback
            };

            println!("select; -> got overlapped");
            // callback(event, status.entry());
        }

        // println!("returning");
        Ok(0)
    }

    pub fn post_accept_event(&mut self, socket: SOCKET) -> io::Result<()> {
        println!("post_accept_event!!!!!!!!!!!! socket = {:?}", socket);
        if let Some(event) = self.event_maps.get_mut(&socket) {
            let event = &mut (*event.inner);
            let addr = event.buffer.socket.local_addr()?;
            event.accept_socket = match addr {
                SocketAddr::V4(..) => TcpSocket::new_v4()?,
                SocketAddr::V6(..) => TcpSocket::new_v6()?,
            };
            unsafe {
                event.buffer.socket.accept_overlapped(&event.accept_socket, &mut event.accept_buf, event.read.as_mut_ptr())?;
            }
        }
        Ok(())
    }

    // fn _check_can_read(socket: &TcpSocket, read: *mut OVERLAPPED) -> io::Result<bool> {
    //     let res = unsafe {
    //         socket.read_overlapped(&mut [], read)
    //     };
    //     println!("res = {:?}", res);
    //     match res {
    //         // Note that `Ok(true)` means that this completed immediately and
    //         // our socket is readable. This typically means that the caller of
    //         // this function (likely `read` above) can try again as an
    //         // optimization and return bytes quickly.
    //         //
    //         // Normally, though, although the read completed immediately
    //         // there's still an IOCP completion packet enqueued that we're going
    //         // to receive.
    //         //
    //         // You can configure this behavior (miow) with
    //         // SetFileCompletionNotificationModes to indicate that `Ok(true)`
    //         // does **not** enqueue a completion packet. (This is the case
    //         // for me.instant_notify)
    //         //
    //         // Note that apparently libuv has scary code to work around bugs in
    //         // `WSARecv` for UDP sockets apparently for handles which have had
    //         // the `SetFileCompletionNotificationModes` function called on them,
    //         // worth looking into!
    //         Ok(Some(_)) => {
    //             return Ok(true)
    //         }
    //         Ok(_) => {
    //             return Ok(false)
    //         }
    //         Err(e) => {
    //             return Err(e);
    //         }
    //     }
    // }
    
    // fn _do_read(buffer: &mut EventBuffer, read: *mut OVERLAPPED) -> io::Result<usize> {
    //     let res = unsafe {
    //         buffer.socket.read_overlapped(&mut buffer.read_cache[..], read)
    //     };
    //     println!("_do_read _____ res = {:?}", res);
    //     match res {
    //         Ok(Some(n)) => {
    //             buffer.read.write(&buffer.read_cache[..n])?;
    //             return Ok(n)
    //         }
    //         Ok(_) => {
    //             unreachable!();
    //         }
    //         Err(e) => {
    //             return Err(e);
    //         }
    //     }
    // }

    fn _do_read_all(&mut self, socket: SOCKET) -> io::Result<usize> {
        let mut read_size = 0;
        if let Some(event) = self.event_maps.get_mut(&socket) {
            let event = &mut (*event.inner);
            if event.is_end {
                return Ok(read_size);
            }
            // event.buffer.is_in_read = true;
            
            println!("ddddddddddddddddddddddd _do_read_all read socket = {:?}", socket);
            let read = event.read.as_mut_ptr();
            let buffer = &mut event.buffer;
                let res = unsafe {
                    buffer.socket.read_overlapped(&mut buffer.read_cache[..], read)?
                };
            loop {
                break;
                // println!("_do_read _____ res = {:?}", res);
                // match res {
                //     Ok(Some(n)) => {
                //         println!("read_cache = {:?}", &buffer.read_cache[..n]);
                //         if n == 0 {
                //             break;
                //         }
                //         read_size += n;
                //         buffer.read.write(&buffer.read_cache[..n])?;
                //     }
                //     Ok(_) => {
                //         break;
                //     }
                //     Err(e) => {
                //         return Err(e);
                //     }
                // }
            }

            // println!("111111111111");
            // loop {

            //     if Self::_check_can_read(&event.buffer.socket, event.read.as_mut_ptr())? {
            //         let read = Self::_do_read(&mut event.buffer, event.read.as_mut_ptr())?;
            //         if read == 0 {
            //             break;
            //         }
            //         read_size += read;
            //     } else {
            //         break;
            //     }
            // }
            // println!("111111111111");
        }
        Ok(read_size)
    }

    pub fn _do_write_all(&mut self, socket: &SOCKET, data: Option<&[u8]>) -> io::Result<()> {
        if let Some(ev) = self.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if data.is_some() {
                event.buffer.write.write(data.unwrap());
            }
            if event.buffer.is_in_write || event.buffer.write.empty() || event.is_end {
                return Ok(());
            }
            // ev.inner.fetch_add();
            let write = event.write.as_mut_ptr();
            let res = unsafe {
                event.buffer.socket.write_overlapped(&event.buffer.write.get_data()[..], write)?
            };

            println!("----------------------write res = {:?}", res);

            match res {
                Some(n) => {
                    event.buffer.write.drain(n);
                    if !event.buffer.write.empty() {
                        event.buffer.is_in_write = true;
                    }
                    return Ok(());
                }
                _ => {
                    return Ok(());
                }
            }
        }
        println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!! {:?}", socket);
        Err(io::Error::new(ErrorKind::Other, "the socket already be remove"))
    }

    pub fn post_read_event(&mut self, socket: SOCKET) -> io::Result<()> {
        if let Some(ev) = self.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if event.is_end {
                return Ok(());
            }
            // ev.inner.fetch_add();
            // event.buffer.is_in_read = true;
            let res = unsafe {
                
            println!("ddddddddddddddddddddddd post_read_event read socket = {:?}", socket);
                event.buffer.socket.read_overlapped(&mut event.buffer.read_cache[..], event.read.as_mut_ptr())?
            };
            match res {
                Some(n) => {
                    // self.port.post_info(0, FLAG_ENDED, event.read.as_mut_ptr())?;
                }
                None => (),
            }
            println!("post_read_event res = {:?}", res);
        }

        // if self._do_read_all(socket)? > 0 {
        //     let overlap = {
        //         let event = self.event_maps.get_mut(&socket).unwrap();
        //         let event = &mut (*event.inner);
        //         event.read.as_mut_ptr()
        //     };
        //     self.port.post_info(0, FLAG_ENDED, overlap)?;
        // }
        Ok(())
    }

    pub fn post_write_event(&mut self, socket: SOCKET) -> io::Result<()> {
        self._do_write_all(&socket, None)
    }

    pub fn check_socket_event(&mut self, socket: SOCKET) -> io::Result<()> {
        if !self.event_maps.contains_key(&socket) {
            return Ok(());
        }

        let flag = {
            let event = &self.event_maps[&socket];
            (event.inner).entry.ev_events
        };

        println!("check_socket_event flag = {:?}", flag);

        if flag.contains(FLAG_ACCEPT) {
            self.post_accept_event(socket)?;
        } else {
            if flag.contains(FLAG_READ) {
                self.post_read_event(socket)?;
            }

            if flag.contains(FLAG_WRITE) {
                self.post_write_event(socket)?;
            }
        }
        Ok(())
    }
    
    pub fn register_socket(event_loop: &mut EventLoop,
                                  buffer: EventBuffer,
                                  entry: EventEntry) -> io::Result<()> {
        let selector = &mut event_loop.selector;
        let socket = buffer.as_raw_socket();
        println!("register_socket socket = {:?}", socket);
        if selector.event_maps.contains_key(&socket) {
            selector.event_maps.remove(&socket);
        }

        selector.port.add_socket(entry.ev_events, &buffer.socket)?;
        let event = Event::new(buffer, entry);
        selector.event_maps.insert(socket, EventImpl::new(event));
        if let Err(e) = selector.check_socket_event(socket) {
            selector.event_maps.remove(&socket);
            return Err(e);
        }
        Ok(())
    }

    pub fn _unregister_socket(event_loop: &mut EventLoop, socket: SOCKET, _flags: EventFlags) -> io::Result<()> {
        println!("unregister_socket socket = {:?} ---------", socket);
        if let Some(mut ev) = event_loop.selector.event_maps.remove(&socket) {
            let event = &mut (*ev.clone().inner);
            let event_clone = &mut (*ev.inner);
            println!("aaaaaaaaaaaaaaaaaaaaaaaaa");
            event.entry.end_cb(event_loop, &mut event_clone.buffer);
        }
        println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!! {:?}", socket);
        // panic!("!11111111111111");
        Ok(())
    }

    pub fn unregister_socket(event_loop: &mut EventLoop, socket: SOCKET, _flags: EventFlags) -> io::Result<()> {
        println!("unregister_socket socket = {:?} ---------", socket);
        event_loop.selector._notify_socket_end(&socket);
        // if event_loop.selector.event_maps.contains_key(&socket) {
        //     event_loop.selector._notify_socket_end(&socket);
        //     //取消注册则关闭socket等待实际的回调结束
        //     // event.buffer.socket.close();
        //     // let event = &mut (*ev.clone().inner);
        //     // let event_clone = &mut (*ev.inner);
        //     // println!("aaaaaaaaaaaaaaaaaaaaaaaaa");
        //     // event.entry.end_cb(event_loop, &mut event_clone.buffer);
        // }
        println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!! {:?}", socket);
        // panic!("!11111111111111");
        Ok(())
    }

    fn _notify_socket_end(&mut self, socket: &SOCKET) -> io::Result<()> {
        if let Some(ev) = self.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if event.is_end {
                return Ok(());
            }
            println!("ddddddddddddddddddddddd _notify_socket_end socket = {:?}", socket);
            event.is_end = true;
            self.port.post_info(0, FLAG_ENDED, event.read.as_mut_ptr())?;
            //取消注册则关闭socket等待实际的回调结束
            // event.buffer.socket.close();
            // let event = &mut (*ev.clone().inner);
            // let event_clone = &mut (*ev.inner);
            // println!("aaaaaaaaaaaaaaaaaaaaaaaaa");
            // event.entry.end_cb(event_loop, &mut event_clone.buffer);
        }
        Ok(())
        // self.port.post_info(0, FLAG_ENDED, event.read.as_mut_ptr())?;
    }

    pub fn send_socket(event_loop: &mut EventLoop, socket: &SOCKET, data: &[u8]) -> io::Result<()> {
        println!("send_socket socket = {:?} ---------", socket);
        event_loop.selector._do_write_all(socket, Some(data))
    }

    pub fn deregister_socket(&mut self, socket: SOCKET) -> io::Result<()> {
        // let socket = buffer.as_raw_socket();
        // println!("socket = {:?}", socket);
        // if self.event_maps.contains_key(&socket) {
        //     self.event_maps.remove(&socket);
        // }
        Ok(())
    }

    pub fn register(&mut self, fd: SOCKET, ev_events: EventFlags) {
        // let fd = fd as SOCKET;
        // if ev_events.contains(FLAG_READ) && !self.read_sockets.contains(&fd) {
        //     self.read_sockets.push(fd);
        // }
        // if ev_events.contains(FLAG_WRITE) && !self.write_sockets.contains(&fd) {
        //     self.write_sockets.push(fd);
        // }
    }

    pub fn deregister(&mut self, socket: SOCKET, _flag: EventFlags) {
        // if let Some(event) = self.event_maps.remove(&socket) {
            
        //     cancel(event.buffer.socket, event.read);
        //     cancel(event.buffer.socket, event.write);
        // }

        // if self.event_maps.contains_key(&socket) {
        //     self.event_maps.remove(&socket);
        // }

        // self.port.add_socket(entry.ev_events, &buffer.socket)?;
        // let event = Event::new(buffer, entry);
        // self.event_maps.insert(socket, EventImpl::new(event));
        // match self.check_socket_event(socket) {
        //     Err(e) => {

        //     }
        //     _ => {

        //     }
        // }
        // Ok(())

        // let fd = fd as SOCKET;
        // fn search_index(vec: &Vec<SOCKET>, value: &SOCKET) -> Option<usize> {
        //     for (i, v) in vec.iter().enumerate() {
        //         if value == v {
        //             return Some(i);
        //         }
        //     }
        //     None
        // };

        // if flag.contains(FLAG_READ) {
        //     if let Some(index) = search_index(&self.read_sockets, &fd) {
        //         self.read_sockets.remove(index);
        //     }
        // }

        // if flag.contains(FLAG_WRITE) {
        //     if let Some(index) = search_index(&self.write_sockets, &fd) {
        //         self.write_sockets.remove(index);
        //     }
        // }
    }
}
