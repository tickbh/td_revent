#![allow(dead_code)]
use std::os::unix::io::RawFd;
use std::sync::Arc;
use std::io::{self, ErrorKind};
use {EventEntry, EventFlags, FLAG_READ, FLAG_WRITE, FLAG_ACCEPT, EventBuffer, EventLoop, RetValue};

use std::collections::HashMap;
use psocket::SOCKET;

use nix::unistd::close;
use nix::sys::epoll::*;
use std::io::prelude::*;

use super::FromRawArc;

pub struct Selector {
    epfd: RawFd,
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
        self.entry.ev_events.contains(FLAG_ACCEPT)
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
                event_loop.unregister_socket(event.as_raw_socket(), EventFlags::all());
            }
            _ => {
                ;
            }
        }
    } else {
        match event.buffer.socket.read(&mut event.buffer.read_cache[..]) {
            Ok(len) => {
                if len <= 0 {
                    Selector::unregister_socket(
                        event_loop,
                        event.buffer.as_raw_socket(),
                        EventFlags::all(),
                    );
                    return;
                }

                let _ = event.buffer.read.write(
                    &event.buffer.read_cache
                        [..len],
                );

                if event.buffer.has_read_buffer() {
                    match event.entry.EventCb(event_loop, &mut event_clone.buffer) {
                        RetValue::OVER => {
                            event_loop.unregister_socket(event.as_raw_socket(), EventFlags::all());
                            return;
                        }
                        _ => (),
                    }
                }
            },
            Err(err) => {
                event.buffer.error = Err(err);
                Selector::unregister_socket(
                    event_loop,
                    event.buffer.as_raw_socket(),
                    EventFlags::all(),
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
        event.entry.ev_events.remove(FLAG_WRITE);
        let _ = event_loop.selector.modregister(event.as_raw_socket(), event.entry.ev_events);
        return;
    }
    match event.buffer.socket.write(&event.buffer.write.get_data()[..]) {
        Ok(len) => {
            if len <= 0 {
                Selector::unregister_socket(
                    event_loop,
                    event.buffer.as_raw_socket(),
                    EventFlags::all(),
                );
                return;
            }
            event.buffer.write.drain(len);
            //如果写入包为空, 则表示没有数据要进行写入, 取消掉写入事件
            if event.buffer.write.empty() {
                event.buffer.is_in_write = false;
                event.entry.ev_events.remove(FLAG_WRITE);
                let _ = event_loop.selector.modregister(event.as_raw_socket(), event.entry.ev_events);
            }
        },
        Err(err) => {
            event.buffer.error = Err(err);
            Selector::unregister_socket(
                event_loop,
                event.buffer.as_raw_socket(),
                EventFlags::all(),
            );
        },
    }
}


impl Selector {
    pub fn new(capacity: usize) -> io::Result<Selector> {
        let epfd = try!(epoll_create());
        Ok(Selector {
            epfd: epfd,
            evts: Events::new(capacity),
            event_maps: HashMap::new(),
        })
    }

    /// 获取当前可执行的事件, 并同时处理数据, 返回执行的个数
    pub fn do_select(event: &mut EventLoop, timeout: usize) -> io::Result<usize> {
        use std::{isize, slice};
        let timeout_ms = if timeout as isize >= isize::MAX {
            isize::MAX
        } else {
            timeout as isize
        };

        let dst = unsafe {
            slice::from_raw_parts_mut(event.selector.evts.events.as_mut_ptr(), event.selector.evts.events.capacity())
        };

        // Wait for epoll events for at most timeout_ms milliseconds
        let cnt = try!(epoll_wait(event.selector.epfd, dst, timeout_ms).map_err(
            super::from_nix_error,
        ));

        unsafe {
            event.selector.evts.events.set_len(cnt);
        }

        for i in 0..cnt {
            let value = event.selector.evts.events[i];
            let mut ev_flag = EventFlags::empty();
            if value.events.contains(EPOLLIN) {
                read_done(event, value.data as SOCKET);
            }
            if value.events.contains(EPOLLOUT) {
                write_done(event, value.data as SOCKET);
            }
        }
        Ok(cnt)
    }


    pub fn select(&mut self, evts: &mut Vec<EventEntry>, timeout_ms: u32) -> io::Result<u32> {
        use std::{isize, slice};

        let timeout_ms = if timeout_ms as isize >= isize::MAX {
            isize::MAX
        } else {
            timeout_ms as isize
        };

        let dst = unsafe {
            slice::from_raw_parts_mut(self.evts.events.as_mut_ptr(), self.evts.events.capacity())
        };

        // Wait for epoll events for at most timeout_ms milliseconds
        let cnt = try!(epoll_wait(self.epfd, dst, timeout_ms).map_err(
            super::from_nix_error,
        ));

        unsafe {
            self.evts.events.set_len(cnt);
        }

        evts.clear();
        for i in 0..cnt {
            let value = self.evts.events[i];
            let mut ev_flag = EventFlags::empty();
            if value.events.contains(EPOLLIN) {
                ev_flag = ev_flag | FLAG_READ;
            }
            if value.events.contains(EPOLLOUT) {
                ev_flag = ev_flag | FLAG_WRITE;
            }
            evts.push(EventEntry::new_evfd(value.data as i32, ev_flag));
        }
        Ok(cnt as u32)
    }

    fn register(&self, socket: SOCKET, ev_events: EventFlags) -> io::Result<()> {

        let info = EpollEvent {
            events: ioevent_to_epoll(ev_events),
            data: socket as u64,
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlAdd, socket as RawFd, &info)
            .map_err(super::from_nix_error)
    }

    fn modregister(&self, socket: SOCKET, ev_events: EventFlags) -> io::Result<()> {

        let info = EpollEvent {
            events: ioevent_to_epoll(ev_events),
            data: socket as u64,
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlMod, socket as RawFd, &info)
            .map_err(super::from_nix_error)
    }

    fn deregister(&self, socket: SOCKET, ev_events: EventFlags) -> io::Result<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(ev_events),
            data: 0,
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlDel, socket as RawFd, &info)
            .map_err(super::from_nix_error)
    }


    /// 注册socket事件, 把socket加入到iocp的监听中, 如果监听错误, 则移除相关的资源
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

        let info = EpollEvent {
            events: ioevent_to_epoll(entry.ev_events),
            data: socket as u64,
        };

        let event = Event::new(buffer, entry);
        selector.event_maps.insert(socket, EventImpl::new(event));

        if let Err(e) = epoll_ctl(selector.epfd, EpollOp::EpollCtlAdd, socket as RawFd, &info)
            .map_err(super::from_nix_error) {
            selector.event_maps.remove(&socket);
            return Err(e);
        }
        Ok(())
    }


    /// 取消某个socket的监听, iocp模式下flags参数无效
    pub fn unregister_socket(
        event_loop: &mut EventLoop,
        socket: SOCKET,
        flags: EventFlags,
    ) -> io::Result<()> {
        if let Some(mut event) = event_loop.selector.event_maps.remove(&socket) {
            let event_clone = &mut (*event.clone().inner);
            let event = &mut (*event.inner);
            event.entry.end_cb(event_loop, &mut event_clone.buffer);
        }
        let _ = event_loop.selector.deregister(socket, flags)?;
        
        
        // let info = EpollEvent {
        //     events: ioevent_to_epoll(_flags),
        //     data: 0,
        // };

        // match epoll_ctl(event_loop.selector.epfd, EpollOp::EpollCtlDel, socket as RawFd, &info)
        //     .map_err(super::from_nix_error) {
        //         Err(e) => {
        //             return Err(e);
        //         }
        //         Ok(_) => {
        //             if let Some(mut event) = event_loop.selector.event_maps.remove(&socket) {
        //                 let event_clone = &mut (*event.clone().inner);
        //                 let event = &mut (*event.inner);
        //                 event.entry.end_cb(event_loop, &mut event_clone.buffer);
        //             }
        //         }
        //     }
        Ok(())
    }

    // 给指定的socket发送数据, 如果不能一次发送完毕则会写入到缓存中, 等待下次继续发送
    // 返回值为指定的当次的写入大小, 如果没有全部写完数据, 则下次写入先写到缓冲中, 等待系统的可写通知
    pub fn send_socket(event_loop: &mut EventLoop, socket: &SOCKET, data: &[u8]) -> io::Result<usize> {
        println!("send socket !!!!!!!!!!!! socket = {:?}", socket);
        if !event_loop.selector.event_maps.contains_key(&socket) {
            return Err(io::Error::new(
                ErrorKind::Other,
                "the socket already be remove",
            ));
        }
        let mut event = event_loop.selector.event_maps.get_mut(&socket).map(|e| e.clone()).unwrap();
        let event = &mut (*event.inner);
        event.buffer.write.write(data);
        println!("aaaaaaaaaaaaaaaa");
        if event.buffer.is_in_write || event.buffer.write.empty() {
            return Ok(0);
        }
        println!("bbbbbbbbbbbbbbbbbbbb");
        event.entry.ev_events.insert(FLAG_WRITE);
        event.buffer.is_in_write = true;
        let err = event_loop.selector.modregister(event.as_raw_socket(), event.entry.ev_events);
        println!("err = {:?}", err);
        println!("register write info = {:?}", socket);
        Ok(0)
    }

    // fn post_write_event(&mut self, socket: &SOCKET, data: Option<&[u8]>) -> io::Result<usize> {
    //     if let Some(event) = self.event_maps.get_mut(&socket) {
    //         let event = &mut (*event.inner);
    //         if data.is_some() {
    //             event.buffer.write.write(data.unwrap());
    //         }
    //         if event.buffer.is_in_write || event.buffer.write.empty() {
    //             return Ok(0);
    //         }
            
    //         event_loop.selector.deregister(event.as_raw_socket(), FLAG_WRITE)?;
    //         return Ok(0);
    //     }
    //     Err(io::Error::new(
    //         ErrorKind::Other,
    //         "the socket already be remove",
    //     ))
    // }

}


fn ioevent_to_epoll(ev_events: EventFlags) -> EpollEventKind {
    let mut kind = EpollEventKind::empty();

    if ev_events.contains(FLAG_READ) {
        kind.insert(EPOLLIN);
    }

    if ev_events.contains(FLAG_WRITE) {
        kind.insert(EPOLLOUT);
    }
    // kind.insert(EPOLLET);
    kind
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = close(self.epfd);
    }
}

pub struct Events {
    events: Vec<EpollEvent>,
}

impl Events {
    pub fn new(capacity: usize) -> Events {
        Events { events: Vec::with_capacity(capacity) }
    }
}
