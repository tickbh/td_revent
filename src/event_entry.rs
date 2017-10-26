pub use {EventFlags, FLAG_TIMEOUT, FLAG_READ, FLAG_WRITE, FLAG_PERSIST, EventLoop};
use std::fmt;
use std::ptr;
use std::cmp::Ordering;
use std::hash::Hash;
use std::hash;
use std::cmp::Ord;
use std::any::Any;
extern crate time;

pub struct EventEntry {
    pub ev_fd: u32,
    pub tick_ms: u64,
    pub tick_step: u64,
    pub ev_events: EventFlags,
    pub call_back: Option<fn(ev: &mut EventLoop, fd: u32, flag: EventFlags, data: Option<&mut Box<Any>>) -> i32>,
    pub data: Option<Box<Any>>,
}

impl EventEntry {
    /// tick_step is us
    pub fn new_timer(tick_step: u64,
                     tick_repeat: bool,
                     call_back: Option<fn(ev: &mut EventLoop,
                                          fd: u32,
                                          flag: EventFlags,
                                          data: Option<&mut Box<Any>>)
                                          -> i32>,
                     data: Option<Box<Any>>)
                     -> EventEntry {
        EventEntry {
            tick_ms: time::precise_time_ns() / 1000 + tick_step,
            tick_step: tick_step,
            ev_events: if tick_repeat {
                FLAG_TIMEOUT | FLAG_PERSIST
            } else {
                FLAG_TIMEOUT
            },
            call_back: call_back,
            data: data,
            ev_fd: 0,
        }
    }

    pub fn new(ev_fd: u32,
               ev_events: EventFlags,
               call_back: Option<fn(ev: &mut EventLoop, fd: u32, flag: EventFlags, data: Option<&mut Box<Any>>)
                                    -> i32>,
               data: Option<Box<Any>>)
               -> EventEntry {
        EventEntry {
            tick_ms: 0,
            tick_step: 0,
            ev_events: ev_events,
            call_back: call_back,
            data: data,
            ev_fd: ev_fd,
        }
    }

    pub fn new_evfd(ev_fd: u32, ev_events: EventFlags) -> EventEntry {
        EventEntry {
            tick_ms: 0,
            tick_step: 0,
            ev_events: ev_events,
            call_back: None,
            data: None,
            ev_fd: ev_fd,
        }
    }

    pub fn callback(&mut self, ev: &mut EventLoop, ev_events: EventFlags) -> i32 {
        if self.call_back.is_none() {
            return 0;
        }
        self.call_back.unwrap()(ev,
                                self.ev_fd,
                                ev_events,
                                self.data.as_mut())
    }
}


impl fmt::Debug for EventEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "ev_fd = {}, tick_ms = {}, tick_step = {}, ev_events = {:?}",
               self.ev_fd,
               self.tick_ms,
               self.tick_step,
               self.ev_events)
    }
}

impl Ord for EventEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.tick_ms.cmp(&self.tick_ms)
    }
}

impl PartialOrd for EventEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(other.tick_ms.cmp(&self.tick_ms))
    }
}

impl PartialEq for EventEntry {
    fn eq(&self, other: &Self) -> bool {
        if self.ev_events.contains(FLAG_TIMEOUT) {
            self.tick_ms == other.tick_ms
        } else {
            self.ev_fd == other.ev_fd
        }

    }
}

impl Eq for EventEntry {}

impl Hash for EventEntry {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        (self.ev_fd).hash(state);
    }
}

impl Drop for EventEntry {
    fn drop(&mut self) {
        use psocket::TcpSocket;
        if self.data.is_some() {
            let listener = self.data.as_ref().unwrap().downcast_ref::<TcpSocket>();
            println!("listener = {:?}", listener);
        } else {
            println!("data is none");
        }
        println!("drop the EventEntry!!! {:?}", self);
    }
}