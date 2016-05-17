use {io, EventSet, PollOpt, Token};
use event::{self, Event};
use nix::unistd::close;
use nix::sys::event::{EventFilter, EventFlag, FilterFlag, KEvent, kqueue, kevent, kevent_ts};
use nix::sys::event::{EV_ADD, EV_CLEAR, EV_DELETE, EV_DISABLE, EV_ENABLE, EV_EOF, EV_ERROR, EV_ONESHOT};
use libc::{timespec, time_t, c_long};
use std::{fmt, slice};
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct Selector {
    kq: RawFd,
    evts: Events
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let kq = try!(kqueue().map_err(super::from_nix_error));

        Ok(Selector {
            kq: kq,
            evts: Events::new()
        })
    }

    pub fn select(&mut self, evts: &mut Vec<EventEntry>, timeout_ms: u32) -> io::Result<u32> {
        use std::{isize, slice};

        let timeout_ms = if timeout_ms as isize >= isize::MAX {
            isize::MAX
        } else {
            timeout_ms as isize
        };

        let cnt = try!(kevent_ts(self.kq, &[], self.evts.as_mut_slice(), timeout)
                                  .map_err(super::from_nix_error));

        evts.clear();
        for i in 0..cnt {
            let e = self.evts.sys_events[i];
            let mut ev_flag = EventFlags::empty();
            if e.filter == EventFilter::EVFILT_READ {
                ev_flag = ev_flag | FLAG_READ;
            }
            if e.filter == EventFilter::EVFILT_WRITE {
                ev_flag = ev_flag | FLAG_WRITE;
            }

            evts.push(EventEntry::new_evfd(e.ident as u32, ev_flag));
        }
        Ok(())
    }

    pub fn register(&mut self, fd: u32, ev_events: EventFlags) -> io::Result<()> {

        self.ev_register(fd, EventFilter::EVFILT_READ, ev_events.contains(FLAG_READ));
        self.ev_register(fd, EventFilter::EVFILT_WRITE, ev_events.contains(FLAG_WRITE));

        self.flush_changes()
    }

    pub fn deregister(&mut self, fd: u32, ev_events: EventFlags) -> io::Result<()> {
        self.ev_push(fd, EventFilter::EVFILT_READ, EV_DELETE);
        self.ev_push(fd, EventFilter::EVFILT_WRITE, EV_DELETE);
        self.flush_changes()
    }


    fn ev_register(&mut self, fd: RawFd, filter: EventFilter, enable : bool) {
        let mut flags = EV_ADD;
        if enable {
            flags = flags | EV_ENABLE;
        } else {
            flags = flags | EV_DISABLE;
        }

        self.ev_push(fd, filter, flags);
    }

    fn ev_push(&mut self, fd: RawFd, filter: EventFilter, flags: EventFlag) {
        self.changes.sys_events.push(
            KEvent {
                ident: fd as ::libc::uintptr_t,
                filter: filter,
                flags: flags,
                fflags: FilterFlag::empty(),
                data: 0,
                udata: 0
            });
    }


    fn flush_changes(&mut self) -> io::Result<()> {
        let result = kevent(self.kq, self.evts.as_slice(), &mut [], 0).map(|_| ())
            .map_err(super::from_nix_error).map(|_| ());

        self.evts.sys_events.clear();
        result
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
    pub fn new() -> Events {
        Events {
            sys_events: Vec::with_capacity(1024),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
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
