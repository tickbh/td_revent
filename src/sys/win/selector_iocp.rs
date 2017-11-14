use {EventEntry, EventFlags, FLAG_READ, FLAG_WRITE, FLAG_ACCEPT, FLAG_ENDED, EventBuffer,
     EventLoop, RetValue};
use std::collections::HashMap;
use std::mem;
use psocket::SOCKET;
use std::cell::UnsafeCell;
use std::io::{self, ErrorKind};
use std::time::Duration;
use sys::win::iocp::{CompletionPort, CompletionStatus};
use sys::win::{FromRawArc, Overlapped};
use super::{TcpSocketExt, AcceptAddrsBuf};
use psocket::{TcpSocket, SocketAddr};
use winapi;
use winapi::*;
use kernel32;
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

pub struct Event {
    pub buffer: EventBuffer,
    pub entry: EventEntry,
    pub read: CbOverlapped,
    pub write: CbOverlapped,
    pub accept_buf: Option<AcceptAddrsBuf>,
    pub accept_socket: Option<TcpSocket>,
    pub is_end: bool,
}

impl Event {
    pub fn is_accept(&self) -> bool {
        self.entry.ev_events.contains(FLAG_ACCEPT)
    }

    pub fn get_event_socket(&self) -> SOCKET {
        self.buffer.socket.as_raw_socket()
    }
}

#[derive(Clone)]
pub struct EventImpl {
    pub inner: FromRawArc<Event>,
}

impl EventImpl {
    pub fn new(event: Event) -> EventImpl {
        EventImpl { inner: FromRawArc::new(event) }
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
    callback: fn(&mut EventLoop, &OVERLAPPED_ENTRY),
}


impl CbOverlapped {
    /// Creates a new `Overlapped` which will invoke the provided `cb` callback
    /// whenever it's triggered.
    ///
    /// The returned `Overlapped` must be used as the `OVERLAPPED` passed to all
    /// I/O operations that are registered with mio's event loop. When the I/O
    /// operation associated with an `OVERLAPPED` pointer completes the event
    /// loop will invoke the function pointer provided by `cb`.
    pub fn new(cb: fn(&mut EventLoop, &OVERLAPPED_ENTRY)) -> CbOverlapped {
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
        unsafe { (*self.inner.get()).raw() }
    }
}

impl Events {
    pub fn with_capacity(cap: usize) -> Events {
        Events {
            statuses: vec![CompletionStatus::zero(); cap].into_boxed_slice(),
        }
    }
}

fn read_done(event_loop: &mut EventLoop, status: &OVERLAPPED_ENTRY) {
    let status = CompletionStatus::from_entry(status);
    let mut event = overlapped2arc!(status.overlapped(), Event, read);
    if event.is_accept() {
        let mut socket = event.accept_socket.take().unwrap();
        let result = event.buffer.socket.accept_complete(&socket).and_then(|()| {
            event.accept_buf.as_ref().unwrap().parse(&event.buffer.socket)
        }).and_then(|buf| {
            buf.remote().ok_or_else(|| {
                io::Error::new(ErrorKind::Other, "could not obtain remote address")
            })
        });
        let ret = match result {
            Ok(remote_addr) => {
                socket.set_peer_addr(remote_addr);
                event.entry.accept_cb(event_loop, Ok(socket))
            }
            Err(e) => event.entry.accept_cb(event_loop, Err(e)),
        };
        match ret {
            RetValue::OVER => {
                event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
            }
            _ => {
                if let Err(err) = event_loop.selector.post_accept_event(
                    event.get_event_socket(),
                )
                {
                    event.buffer.error = Err(err);
                    event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
                }
            }
        }
        return;
    } else {
        let mut bytes_transferred = status.bytes_transferred() as usize;
        println!("read_callback!!!!!!! read size = {:?}", bytes_transferred);
        if status.flag().contains(FLAG_ENDED) {
            Selector::_unregister_socket(
                event_loop,
                event.buffer.as_raw_socket(),
                EventFlags::all(),
            );
            return;
        }

        if bytes_transferred == 0 {
            Selector::unregister_socket(
                event_loop,
                event.buffer.as_raw_socket(),
                EventFlags::all(),
            );
            return;
        }
        let mut event_clone = event.clone();
        if bytes_transferred > 0 {
            let _ = event.buffer.read.write(
                &event_clone.buffer.read_cache
                    [..bytes_transferred],
            );
        }
        let res = event_loop.selector.post_read_event(
            &event.get_event_socket(),
        );
        if event.buffer.has_read_buffer() {
            match event.entry.EventCb(event_loop, &mut event_clone.buffer) {
                RetValue::OVER => {
                    event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
                    return;
                }
                _ => (),
            }
        }
        if res.is_err() {
            event.buffer.error = res;
            event_loop.unregister_socket(event.get_event_socket(), EventFlags::all());
        }
    }
}

fn write_done(event_loop: &mut EventLoop, status: &OVERLAPPED_ENTRY) {
    let status = CompletionStatus::from_entry(status);
    let mut event = overlapped2arc!(status.overlapped(), Event, write);
    event.buffer.is_in_write = false;

    println!("write_done = {:?}", event.buffer.as_raw_socket());
    event_loop.selector.post_write_event(
        &event.buffer.as_raw_socket(),
        None,
    );
}

impl Event {
    pub fn new(buffer: EventBuffer, entry: EventEntry) -> Event {
        Event {
            buffer: buffer,
            entry: entry,
            read: CbOverlapped::new(read_done),
            write: CbOverlapped::new(write_done),
            accept_socket: None,
            accept_buf: Some(AcceptAddrsBuf::new()),
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

    pub fn do_select(event: &mut EventLoop, timeout: u32) -> io::Result<u32> {
        let n = match event.selector.port.get_many(
            &mut event.selector.events.statuses,
            Some(Duration::from_millis(timeout as u64)),
        ) {
            Ok(statuses) => statuses.len(),
            Err(ref e) if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) => 0,
            Err(e) => return Err(e),
        };

        let statuses = event.selector.events.statuses[..n].to_vec();
        if n > 0 {
            println!("0000000000do_select n = {:?}", n);
            // for status in &statuses {
            //     if status.overlapped() as usize == 0 {
            //         println!("empty!!!!!!!!!!!!!");
            //         continue;
            //     }
            //     // let status = CompletionStatus::from_entry(status);
            //     let mut event = if status.flag().contains(FLAG_READ) {
            //         overlapped2arc!(status.overlapped(), Event, read)
            //     } else {
            //         overlapped2arc!(status.overlapped(), Event, write)
            //     };

            //     println!("socket is = {:?} flag = {:?}", event.buffer.as_raw_socket(), status.flag());
            //     mem::forget(event);
            // }
        }

        for status in statuses {
            // This should only ever happen from the awakener, and we should
            // only ever have one awakener right now, so assert as such.
            if status.overlapped() as usize == 0 {
                continue;
            }

            let callback = unsafe { (*(status.overlapped() as *mut CbOverlapped)).callback };
            callback(event, status.entry());
        }
        Ok(0)
    }

    pub fn post_accept_event(&mut self, socket: SOCKET) -> io::Result<()> {
        if let Some(event) = self.event_maps.get_mut(&socket) {
            let event = &mut (*event.inner);
            let addr = event.buffer.socket.local_addr()?;
            event.accept_socket = Some(match addr {
                SocketAddr::V4(..) => TcpSocket::new_v4()?,
                SocketAddr::V6(..) => TcpSocket::new_v6()?,
            });
            unsafe {
                event.buffer.socket.accept_overlapped(
                    &event.accept_socket.as_ref().unwrap(),
                    event.accept_buf.as_mut().unwrap(),
                    event.read.as_mut_ptr(),
                )?;
            }
        }
        Ok(())
    }

    pub fn post_read_event(&mut self, socket: &SOCKET) -> io::Result<()> {
        if let Some(ev) = self.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if event.is_end {
                return Ok(());
            }
            println!("0000000000post_read_event!!!!!!!! socket = {:?}", socket);
            unsafe {
                event.buffer.socket.read_overlapped(
                    &mut event.buffer.read_cache[..],
                    event.read.as_mut_ptr(),
                )?
            };
        }
        Ok(())
    }

    pub fn post_write_event(&mut self, socket: &SOCKET, data: Option<&[u8]>) -> io::Result<usize> {
        if let Some(ev) = self.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if data.is_some() {
                println!("post_write_event data size = {:?}", data.unwrap().len());
                event.buffer.write.write(data.unwrap());
            }
            if event.buffer.is_in_write || event.buffer.write.empty() || event.is_end {
                return Ok(0);
            }

            println!("0000000000post_write_event!!!!!!!! socket = {:?}", socket);
            let write = event.write.as_mut_ptr();
            let res = unsafe {
                event.buffer.socket.write_overlapped(
                    &event.buffer.write.get_data()[..],
                    write,
                )?
            };

            println!("post_write_event len = {} write = {:?}", event.buffer.write.get_data().len(), res);

            match res {
                Some(n) => {

                    println!("write success size = {:?} write len = {:?}", n, event.buffer.write.len());
                    event.buffer.write.drain(n);
                    println!("after len = {:?}", event.buffer.write.len());
                    //如果写入包没有写入完毕, 则表示iocp已被填满, 如果下次写入的将等待write事件返回再次写入
                    if !event.buffer.write.empty() {
                        event.buffer.is_in_write = true;
                    }
                    return Ok(n);
                }
                _ => {
                    return Ok(0);
                }
            }
        }
        Err(io::Error::new(
            ErrorKind::Other,
            "the socket already be remove",
        ))
    }

    pub fn check_socket_event(&mut self, socket: SOCKET) -> io::Result<()> {
        if !self.event_maps.contains_key(&socket) {
            return Ok(());
        }

        let flag = {
            let event = &self.event_maps[&socket];
            (event.inner).entry.ev_events
        };

        if flag.contains(FLAG_ACCEPT) {
            self.post_accept_event(socket)?;
        } else {
            if flag.contains(FLAG_READ) {
                self.post_read_event(&socket)?;
            }
            if flag.contains(FLAG_WRITE) {
                self.post_write_event(&socket, None)?;
            }
        }
        Ok(())
    }

    pub fn register_socket(
        event_loop: &mut EventLoop,
        buffer: EventBuffer,
        entry: EventEntry,
    ) -> io::Result<()> {
        let selector = &mut event_loop.selector;
        let socket = buffer.as_raw_socket();

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

    pub fn _unregister_socket(
        event_loop: &mut EventLoop,
        socket: SOCKET,
        _flags: EventFlags,
    ) -> io::Result<()> {
        if let Some(mut ev) = event_loop.selector.event_maps.remove(&socket) {
            let event = &mut (*ev.clone().inner);
            let event_clone = &mut (*ev.inner);
            event.entry.end_cb(event_loop, &mut event_clone.buffer);
        }
        Ok(())
    }

    pub fn unregister_socket(
        event_loop: &mut EventLoop,
        socket: SOCKET,
        _flags: EventFlags,
    ) -> io::Result<()> {
        if let Some(ev) = event_loop.selector.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if event.is_end {
                return Ok(());
            }
            // 必须先主动关闭掉socket保证iocp里面的read和write事件先被唤醒
            // 然后才唤醒FLAG_ENDED事件, 进行最终析构, 确保资源正确的释放
            event.buffer.socket.close();
            event.is_end = true;
            println!("0000000000post info!!!!!!!!!!!!!!!! socket = {:?}", event.buffer.as_raw_socket());
            event_loop.selector.port.post_info(0, FLAG_ENDED, event.read.as_mut_ptr())?;
        }
        Ok(())
    }

    pub fn send_socket(event_loop: &mut EventLoop, socket: &SOCKET, data: &[u8]) -> io::Result<usize> {
        println!("send_socket = {:?} data = {:?}", socket, data);
        event_loop.selector.post_write_event(socket, Some(data))
    }
}
