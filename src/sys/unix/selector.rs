
use libc;
use super::epoll::*;
use std::io;
use std::os::unix::io::RawFd;
use {EventEntry, EventFlags, FLAG_READ, FLAG_WRITE};


pub struct Selector {
    epfd : RawFd,
    evts : Events,
}

fn from_sys_error(_ : io::Error) -> io::Error {
    io::Error::last_os_error()
}

impl Selector {
    
    pub fn new() -> io::Result<Selector> {
        let epfd = try!(epoll_create());
        Ok(Selector {
            epfd : epfd,
            evts : Events::new(),
        })
    }

    pub fn select(&mut self, evts : &mut Vec<EventEntry>, timeout_ms : u32) -> io::Result<u32> {
        use std::{isize, slice};

        let timeout_ms = if timeout_ms as isize >= isize::MAX {
            isize::MAX
        } else {
            timeout_ms as isize
        };

        let dst = unsafe {
            slice::from_raw_parts_mut(
                self.evts.events.as_mut_ptr(),
                self.evts.events.capacity())
        };

        // Wait for epoll events for at most timeout_ms milliseconds
        let cnt = try!(epoll_wait(self.epfd, dst, timeout_ms)
                           .map_err(from_sys_error));
        if cnt > 0  {
            println!("select is {}", cnt);    
        }
        
        unsafe { self.evts.events.set_len(cnt); }

        evts.clear();
        for i in 0 .. cnt {
            let value = self.evts.events[i];
            let mut ev_flag = EventFlags::empty();
            if value.events.contains(EPOLLIN) {
                ev_flag = ev_flag | FLAG_READ;
            }
            if value.events.contains(EPOLLOUT) {
                ev_flag = ev_flag | FLAG_WRITE;
            }
            evts.push(EventEntry::new_evfd(value.data, ev_flag));
        }
        Ok(cnt as u32)
    }

    pub fn register(&mut self, fd : u64, ev_events : EventFlags) -> io::Result<()>  {

        let info = EpollEvent {
            events: ioevent_to_epoll(ev_events),
            data: fd as u64
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlAdd, fd as i32, &info)
            .map_err(from_sys_error)
    }

    pub fn deregister(&mut self, fd : u64, _ev_events : EventFlags) -> io::Result<()>  {
        let info = EpollEvent {
            events: EpollEventKind::empty(),
            data: 0
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlDel, fd as RawFd, &info).map_err(from_sys_error)
    }
}


fn ioevent_to_epoll(ev_events : EventFlags) -> EpollEventKind {
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
        let _ = unsafe { libc::close(self.epfd) } ;
    }
}

pub struct Events {
    events: Vec<EpollEvent>,
}

impl Events {
    pub fn new() -> Events {
        Events {
            events: Vec::with_capacity(1024),
        }
    }
}
