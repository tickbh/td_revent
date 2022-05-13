#![allow(dead_code)]
use std::os::unix::io::RawFd;
use std::io::{self, ErrorKind};
use {EventEntry, EventFlags, EventBuffer, EventLoop, RetValue};

use libc::{timespec, time_t, c_long};

use std::collections::HashMap;
use psocket::SOCKET;
use std::{fmt, slice};

use nix::unistd::close;
use nix::sys::event::*;
use std::io::prelude::*;

use super::FromRawArc;

pub struct Selector {
    kq: RawFd,
    evts: Events,
    event_maps: HashMap<SOCKET, EventImpl>,
}

pub struct Event {
    pub buffer: EventBuffer,
    pub entry: EventEntry,
    pub is_end: bool,
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

impl Event {
    pub fn new(buffer: EventBuffer, entry: EventEntry) -> Event {
        Event {
            buffer: buffer,
            entry: entry,
            is_end: false,
        }
    }

    pub fn is_accept(&self) -> bool {
        self.entry.ev_events.contains(EventFlags::FLAG_ACCEPT)
    }

    pub fn as_raw_socket(&self) -> SOCKET {
        self.buffer.socket.as_raw_socket()
    }
}

fn read_done(event_loop: &mut EventLoop, socket: SOCKET) {
    if !event_loop.selector.event_maps.contains_key(&socket) {
        return;
    }
    let mut event = event_loop.selector.event_maps.get_mut(&socket).map(|e| e.clone()).unwrap();
    let event_clone = &mut (*event.clone().inner);
    let event = &mut (*event.inner);
    if event.is_accept() {
        let ret = match event.buffer.socket.accept() {
            Ok((mut socket, addr)) => {
                socket.set_peer_addr(addr);
                event.entry.accept_cb(event_loop, Ok(socket))
            },
            Err(e) => {
                event.entry.accept_cb(event_loop, Err(e))
            }
        };

        match ret {
            RetValue::OVER => {
                let _ = event_loop.unregister_socket(event.as_raw_socket());
            }
            _ => {
                ;
            }
        }
    } else {
        match event.buffer.socket.read(&mut event.buffer.read_cache[..]) {
            Ok(len) => {
                if len <= 0 {
                    let _ = Selector::unregister_socket(
                        event_loop,
                        event.buffer.as_raw_socket()
                    );
                    return;
                }

                let _ = event.buffer.read.write(
                    &event.buffer.read_cache
                        [..len],
                );

                if event.buffer.has_read_buffer() {
                    match event.entry.read_cb(event_loop, &mut event_clone.buffer) {
                        RetValue::OVER => {
                            let _ = event_loop.unregister_socket(event.as_raw_socket());
                            return;
                        }
                        _ => (),
                    }
                }
            },
            Err(err) => {
                event.buffer.error = Err(err);
                let _ = Selector::unregister_socket(
                    event_loop,
                    event.buffer.as_raw_socket()
                );
            },
        };
    }
}

fn write_done(event_loop: &mut EventLoop, socket: SOCKET) {
    if !event_loop.selector.event_maps.contains_key(&socket) {
        return;
    }
    let mut event = event_loop.selector.event_maps.get_mut(&socket).map(|e| e.clone()).unwrap();
    let event = &mut (*event.inner);
    // 无需写入, 则取消写入事件
    if event.buffer.write.len() == 0 {
        event.buffer.is_in_write = false;
        event.entry.ev_events.remove(EventFlags::FLAG_WRITE);
        let _ = event_loop.selector.deregister(event.as_raw_socket(), EventFlags::FLAG_WRITE);
        return;
    }
    match event.buffer.socket.write(&event.buffer.write.get_data()[..]) {
        Ok(len) => {
            if len <= 0 {
                let _ = Selector::unregister_socket(
                    event_loop,
                    event.buffer.as_raw_socket()
                );
                return;
            }
            event.buffer.write.drain(len);
            //如果写入包为空, 则表示没有数据要进行写入, 取消掉写入事件
            if event.buffer.write.empty() {
                event.buffer.is_in_write = false;
                event.entry.ev_events.remove(EventFlags::FLAG_WRITE);
                let _ = event_loop.selector.deregister(event.as_raw_socket(), EventFlags::FLAG_WRITE);
            }
        },
        Err(err) => {
            event.buffer.error = Err(err);
            let _ = Selector::unregister_socket(
                event_loop,
                event.buffer.as_raw_socket()
            );
        },
    }
}


impl Selector {
    pub fn new(capacity: usize) -> io::Result<Selector> {
        let kq = try!(kqueue().map_err(super::from_nix_error));

        Ok(Selector {
            kq: kq,
            evts: Events::new(capacity),
            event_maps: HashMap::new(),
        })
    }


    /// 获取当前可执行的事件, 并同时处理数据, 返回执行的个数
    pub fn do_select(event: &mut EventLoop, timeout: usize) -> io::Result<usize> {

        use std::isize;

        let timeout_ms = if timeout as isize >= isize::MAX {
            isize::MAX
        } else {
            timeout as isize
        };

        let timeout = timespec {
            tv_sec: (timeout_ms / 1000) as time_t,
            tv_nsec: ((timeout_ms % 1000) * 1_000_000) as c_long,
        };

        let cnt = try!(
            kevent_ts(event.selector.kq, &[], event.selector.evts.as_mut_slice(), Some(timeout))
                .map_err(super::from_nix_error)
        );
        unsafe {
            event.selector.evts.sys_events.set_len(cnt);
        }

        for i in 0..cnt {
            let e = event.selector.evts.sys_events[i];
            if e.filter()? == EventFilter::EVFILT_READ {
                read_done(event, e.ident() as SOCKET);
            }
            if e.filter()? == EventFilter::EVFILT_WRITE {
                write_done(event, e.ident() as SOCKET);
            }
        }
        Ok(cnt)
    }



    pub fn select(&mut self, evts: &mut Vec<EventEntry>, timeout_ms: u32) -> io::Result<u32> {
        use std::isize;

        let timeout_ms = if timeout_ms as isize >= isize::MAX {
            isize::MAX
        } else {
            timeout_ms as isize
        };

        let timeout = timespec {
            tv_sec: (timeout_ms / 1000) as time_t,
            tv_nsec: ((timeout_ms % 1000) * 1_000_000) as c_long,
        };

        let cnt = try!(
            kevent_ts(self.kq, &[], self.evts.as_mut_slice(), Some(timeout))
                .map_err(super::from_nix_error)
        );
        unsafe {
            self.evts.sys_events.set_len(cnt);
        }

        evts.clear();
        for i in 0..cnt {
            let e = self.evts.sys_events[i];
            let mut ev_flag = EventFlags::empty();
            if e.filter()? == EventFilter::EVFILT_READ {
                ev_flag = ev_flag | EventFlags::FLAG_READ;
            }
            if e.filter()? == EventFilter::EVFILT_WRITE {
                ev_flag = ev_flag | EventFlags::FLAG_WRITE;
            }

            evts.push(EventEntry::new_evfd(e.ident() as i32, ev_flag));
        }
        Ok(cnt as u32)
    }

    fn ev_register(&mut self, fd: RawFd, filter: EventFilter, enable: bool) {
        let mut flags = EventFlag::EV_ADD;
        if enable {
            flags = flags | EventFlag::EV_ENABLE;
        } else {
            flags = flags | EventFlag::EV_DISABLE;
        }

        self.ev_push(fd, filter, flags);
    }

    fn ev_push(&mut self, fd: RawFd, filter: EventFilter, flags: EventFlag) {
        self.evts.sys_events.push(KEvent::new(fd as ::libc::uintptr_t, filter, flags, FilterFlag::empty(), 0, 0));
    }

    fn flush_changes(&mut self) -> io::Result<()> {
        let result = kevent(self.kq, self.evts.as_slice(), &mut [], 0)
            .map(|_| ())
            .map_err(super::from_nix_error)
            .map(|_| ());

        self.evts.sys_events.clear();
        result
    }

    fn register(&mut self, socket: SOCKET, ev_events: EventFlags) -> io::Result<()> {
        if ev_events.contains(EventFlags::FLAG_READ) {
            self.ev_register(
                socket as RawFd,
                EventFilter::EVFILT_READ,
                true,
            );
        }  

        if ev_events.contains(EventFlags::FLAG_WRITE) {
            self.ev_register(
                socket as RawFd,
                EventFilter::EVFILT_WRITE,
                true,
            );
        }
        self.flush_changes()
    }


    fn deregister(&mut self, socket: SOCKET, ev_events: EventFlags) -> io::Result<()> {
        if ev_events.contains(EventFlags::FLAG_READ) {
            self.ev_push(socket as RawFd, EventFilter::EVFILT_READ, EventFlag::EV_DELETE);
        }
        if ev_events.contains(EventFlags::FLAG_WRITE) {
            self.ev_push(socket as RawFd, EventFilter::EVFILT_WRITE, EventFlag::EV_DELETE);
        }
        self.flush_changes()
    }


    /// 注册socket事件, 把socket加入到kqueue的监听中, 如果监听错误, 则移除相关的资源
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

        let events = entry.ev_events.clone();
        let event = Event::new(buffer, entry);
        selector.event_maps.insert(socket, EventImpl::new(event));

        if let Err(e) = selector.register(socket as RawFd, events) {
            selector.event_maps.remove(&socket);
            return Err(e);
        }
        Ok(())
    }

    
    /// 注册socket事件, 把socket加入到kqueue的监听中, 如果监听错误, 则移除相关的资源
    pub fn modify_socket(
        event_loop: &mut EventLoop,
        is_del: bool,
        socket: SOCKET,
        entry: EventEntry,
    ) -> io::Result<()> {

        let err = {
            let selector = &mut event_loop.selector;
            if !selector.event_maps.contains_key(&socket) {
                return Ok(())
            }

            let ev_events = {
                let ev = &selector.event_maps[&socket];
                let event = &mut (*ev.clone().inner);
                event.entry.merge(is_del, entry);
                event.entry.ev_events
            };

            if let Err(e) = selector.register(socket, ev_events) {
                Err(e)
            } else {
                return Ok(())
            }
        };
        Self::unregister_socket(event_loop, socket)?;
        return err;
    }

    /// 取消某个socket的监听
    pub fn unregister_socket(
        event_loop: &mut EventLoop,
        socket: SOCKET
    ) -> io::Result<()> {
        if let Some(mut event) = event_loop.selector.event_maps.remove(&socket) {
            let event_clone = &mut (*event.clone().inner);
            let event = &mut (*event.inner);
            event_clone.buffer.socket.close();
            event.entry.end_cb(event_loop, &mut event_clone.buffer);
        }
        let _ = event_loop.selector.deregister(socket, EventFlags::all())?;
        
        Ok(())
    }

    // 给指定的socket发送数据, 如果不能一次发送完毕则会写入到缓存中, 等待下次继续发送
    // 返回值为指定的当次的写入大小, 如果没有全部写完数据, 则下次写入先写到缓冲中, 等待系统的可写通知
    pub fn send_socket(event_loop: &mut EventLoop, socket: &SOCKET, data: &[u8]) -> io::Result<usize> {
        if !event_loop.selector.event_maps.contains_key(&socket) {
            return Err(io::Error::new(
                ErrorKind::Other,
                "the socket already be remove",
            ));
        }
        let mut event = event_loop.selector.event_maps.get_mut(&socket).map(|e| e.clone()).unwrap();
        let event = &mut (*event.inner);
        event.buffer.write.write(data)?;
        if event.buffer.is_in_write || event.buffer.write.empty() {
            return Ok(0);
        }
        event.entry.ev_events.insert(EventFlags::FLAG_WRITE);
        event.buffer.is_in_write = true;
        event_loop.selector.register(event.as_raw_socket(), EventFlags::FLAG_WRITE)?;
        Ok(0)
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = close(self.kq);
    }
}

pub struct Events {
    sys_events: Vec<KEvent>,
}

impl Events {
    pub fn new(capacity: usize) -> Events {
        Events { sys_events: Vec::with_capacity(capacity) }
    }

    fn as_slice(&self) -> &[KEvent] {
        unsafe {
            let ptr = (&self.sys_events[..]).as_ptr();
            slice::from_raw_parts(ptr, self.sys_events.len())
        }
    }

    fn as_mut_slice(&mut self) -> &mut [KEvent] {
        unsafe {
            let ptr = (&mut self.sys_events[..]).as_mut_ptr();
            slice::from_raw_parts_mut(ptr, self.sys_events.capacity())
        }
    }
}

impl fmt::Debug for Events {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Events {{ len: {} }}", self.sys_events.len())
    }
}
