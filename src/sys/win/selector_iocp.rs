use {EventEntry, EventFlags, FLAG_READ, FLAG_WRITE, FLAG_ACCEPT, EventBuffer, EventLoop};
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
use ws2_32::*;


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

fn accept_done(event: &mut EventLoop, status: &OVERLAPPED_ENTRY) {
    println!("1111111111111111");
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
}


fn read_done(event_loop: &mut EventLoop, status: &OVERLAPPED_ENTRY) {
    println!("1111111111111111");
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
        match result {
            Ok(remote_addr) => socket.set_peer_addr(remote_addr),
            Err(e) => {
                
            },
        };

        println!("socket new is = {:?}", socket);
        event_loop.selector.post_accept_event(event.get_event_socket());
    }
    // let status = CompletionStatus::from_entry(status);
    // let me2 = StreamImp {
    //     inner: unsafe { overlapped2arc!(status.overlapped(), StreamIo, read) },
    // };

    // let mut me = me2.inner();
    // match mem::replace(&mut me.read, State::Empty) {
    //     State::Pending(()) => {
    //         trace!("finished a read: {}", status.bytes_transferred());
    //         assert_eq!(status.bytes_transferred(), 0);
    //         me.read = State::Ready(());
    //         return me2.add_readiness(&mut me, Ready::readable())
    //     }
    //     s => me.read = s,
    // }

    // // If a read didn't complete, then the connect must have just finished.
    // trace!("finished a connect");

    // // By guarding with socket.result(), we ensure that a connection
    // // was successfully made before performing operations requiring a
    // // connected socket.
    // match unsafe { me2.inner.socket.result(status.overlapped()) }
    //     .and_then(|_| me2.inner.socket.connect_complete())
    // {
    //     Ok(()) => {
    //         me2.add_readiness(&mut me, Ready::writable());
    //         me2.schedule_read(&mut me);
    //     }
    //     Err(e) => {
    //         me2.add_readiness(&mut me, Ready::readable() | Ready::writable());
    //         me.read = State::Error(e);
    //     }
    // }
}

fn write_done(event: &mut EventLoop, status: &OVERLAPPED_ENTRY) {
    // let status = CompletionStatus::from_entry(status);
    // trace!("finished a write {}", status.bytes_transferred());
    // let me2 = StreamImp {
    //     inner: unsafe { overlapped2arc!(status.overlapped(), StreamIo, write) },
    // };
    // let mut me = me2.inner();
    // let (buf, pos) = match mem::replace(&mut me.write, State::Empty) {
    //     State::Pending(pair) => pair,
    //     _ => unreachable!(),
    // };
    // let new_pos = pos + (status.bytes_transferred() as usize);
    // if new_pos == buf.len() {
    //     me2.add_readiness(&mut me, Ready::writable());
    // } else {
    //     me2.schedule_write(buf, new_pos, &mut me);
    // }
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

        let statuses = event.selector.events.statuses[..n].to_vec();
        let mut ret = false;
        for status in statuses {
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
            callback(event, status.entry());
        }

        // println!("returning");
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

    pub fn post_read_event(&mut self, socket: SOCKET) -> io::Result<()> {
        if let Some(event) = self.event_maps.get_mut(&socket) {
            let event = &mut (*event.inner);
            if event.buffer.is_in_read {
                return Ok(());
            }
            event.buffer.is_in_read = true;
            unsafe {
                event.buffer.socket.read_overlapped(&mut event.buffer.read_cache[..], event.read.as_mut_ptr())?;
            }
        }
        Ok(())
    }

    pub fn post_write_event(&mut self, socket: SOCKET) -> io::Result<()> {
        if let Some(event) = self.event_maps.get_mut(&socket) {
            let event = &mut (*event.inner);
            if event.buffer.is_in_write || event.buffer.write.empty() {
                return Ok(());
            }
            event.buffer.is_in_write = true;
            let ret = unsafe {
                event.buffer.socket.write_overlapped(&event.buffer.write.get_data()[..], event.write.as_mut_ptr())
            };
            match ret {
                Ok(Some(transferred_bytes)) => {
                    event.buffer.write.drain(transferred_bytes);
                }
                Ok(_) => {

                }
                Err(e) => {
                    // trace!("write error: {}", e);
                    // me.write = State::Error(e);
                    // self.add_readiness(me, Ready::writable());
                    // me.iocp.put_buffer(buf);
                    // break;
                }
            }
        }
        Ok(())
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
                self.post_read_event(socket)?;
            }

            if flag.contains(FLAG_WRITE) {
                self.post_write_event(socket)?;
            }
        }

        Ok(())
    }
    
    pub unsafe fn register_socket(&mut self,
                                  buffer: EventBuffer,
                                  entry: EventEntry) -> io::Result<()> {
        let socket = buffer.as_raw_socket();
        println!("socket = {:?}", socket);
        if self.event_maps.contains_key(&socket) {
            self.event_maps.remove(&socket);
        }

        self.port.add_socket(entry.ev_events, &buffer.socket)?;
        let event = Event::new(buffer, entry);
        self.event_maps.insert(socket, EventImpl::new(event));
        self.check_socket_event(socket);
        Ok(())
    }

    pub fn register(&mut self, fd: i32, ev_events: EventFlags) {
        // let fd = fd as SOCKET;
        // if ev_events.contains(FLAG_READ) && !self.read_sockets.contains(&fd) {
        //     self.read_sockets.push(fd);
        // }
        // if ev_events.contains(FLAG_WRITE) && !self.write_sockets.contains(&fd) {
        //     self.write_sockets.push(fd);
        // }
    }

    pub fn deregister(&mut self, fd: i32, flag: EventFlags) {
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
