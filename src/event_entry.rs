pub use {EventFlags, FLAG_TIMEOUT, FLAG_READ, FLAG_WRITE, FLAG_ACCEPT, FLAG_PERSIST, EventLoop, RetValue,
         EventBuffer};
use std::fmt;
use std::cell::Cell;
use std::cmp::{Ord, Ordering};
use std::hash::{self, Hash};
use std::any::Any;
use std::io::Result;
use psocket::{TcpSocket, SOCKET, INVALID_SOCKET};

extern crate time;

pub type AcceptCb = fn(ev: &mut EventLoop,
                       Result<TcpSocket>,
                       data: Option<&mut Box<Any>>)
                       -> RetValue;
pub type EventCb = fn(ev: &mut EventLoop,
                      &mut EventBuffer,
                      data: Option<&mut Box<Any>>)
                      -> RetValue;
pub type EndCb = fn(ev: &mut EventLoop, &mut EventBuffer, data: Option<Box<Any>>);
pub type TimerCb = fn(ev: &mut EventLoop, timer: u32, data: Option<&mut Box<Any>>) -> RetValue;

pub struct EventEntry {
    pub ev_fd: SOCKET,
    pub time_id: u32,
    pub tick_ms: u64,
    pub tick_step: u64,
    pub ev_events: EventFlags,
    pub accept: Option<AcceptCb>,
    pub read: Option<EventCb>,
    pub write: Option<EventCb>,
    pub end: Option<EndCb>,
    pub timer: Option<TimerCb>,
    pub data: Option<Box<Any>>,
}

impl EventEntry {
    /// tick_step is us
    pub fn new_timer(
        tick_step: u64,
        tick_repeat: bool,
        timer: Option<TimerCb>,
        data: Option<Box<Any>>,
    ) -> EventEntry {
        EventEntry {
            tick_ms: time::precise_time_ns() / 1000 + tick_step,
            tick_step: tick_step,
            ev_events: if tick_repeat {
                FLAG_TIMEOUT | FLAG_PERSIST
            } else {
                FLAG_TIMEOUT
            },
            accept: None,
            read: None,
            write: None,
            end: None,
            timer: timer,
            data: data,
            time_id: 0,
            ev_fd: INVALID_SOCKET,
        }
    }

    /// tick_step is us
    pub fn new_timer_at(
        tick_time: u64,
        timer: Option<TimerCb>,
        data: Option<Box<Any>>,
    ) -> EventEntry {
        EventEntry {
            tick_ms: tick_time,
            tick_step: 0,
            ev_events: FLAG_TIMEOUT,
            accept: None,
            read: None,
            write: None,
            end: None,
            timer: timer,
            data: data,
            time_id: 0,
            ev_fd: INVALID_SOCKET,
        }
    }

    pub fn new_event(
        ev_fd: SOCKET,
        ev_events: EventFlags,
        read: Option<EventCb>,
        write: Option<EventCb>,
        end: Option<EndCb>,
        data: Option<Box<Any>>,
    ) -> EventEntry {
        EventEntry {
            tick_ms: 0,
            tick_step: 0,
            ev_events: ev_events,
            accept: None,
            read: read,
            write: write,
            end: end,
            timer: None,
            data: data,
            time_id: 0,
            ev_fd: ev_fd,
        }
    }

    pub fn new_accept(
        ev_fd: SOCKET,
        ev_events: EventFlags,
        accept: Option<AcceptCb>,
        end: Option<EndCb>,
        data: Option<Box<Any>>,
    ) -> EventEntry {
        EventEntry {
            tick_ms: 0,
            tick_step: 0,
            ev_events: ev_events,
            accept: accept,
            read: None,
            write: None,
            end: end,
            timer: None,
            data: data,
            time_id: 0,
            ev_fd: ev_fd,
        }
    }

    pub fn new_evfd(ev_fd: SOCKET, ev_events: EventFlags) -> EventEntry {
        EventEntry {
            tick_ms: 0,
            tick_step: 0,
            ev_events: ev_events,
            accept: None,
            read: None,
            write: None,
            end: None,
            timer: None,
            data: None,
            time_id: 0,
            ev_fd: ev_fd,
        }
    }

    pub fn accept_cb(&mut self, ev: &mut EventLoop, tcp: Result<TcpSocket>) -> RetValue {
        if self.accept.is_none() {
            return RetValue::OK;
        }

        self.accept.unwrap()(ev, tcp, self.data.as_mut())
    }

    pub fn read_cb(&mut self, ev: &mut EventLoop, event: &mut EventBuffer) -> RetValue {
        if self.read.is_none() {
            return RetValue::OK;
        }

        self.read.unwrap()(ev, event, self.data.as_mut())
    }
    
    pub fn write_cb(&mut self, ev: &mut EventLoop, event: &mut EventBuffer) -> RetValue {
        if self.write.is_none() {
            return RetValue::OK;
        }

        self.write.unwrap()(ev, event, self.data.as_mut())
    }

    pub fn timer_cb(&mut self, ev: &mut EventLoop, timer: u32) -> RetValue {
        if self.timer.is_none() {
            return RetValue::OK;
        }

        self.timer.unwrap()(ev, timer, self.data.as_mut())
    }

    pub fn end_cb(&mut self, ev: &mut EventLoop, event: &mut EventBuffer) {
        if self.end.is_none() {
            return;
        }

        self.end.unwrap()(ev, event, self.data.take())
    }

    pub fn has_flag(&self, flag: EventFlags) -> bool {
        self.ev_events.contains(flag)
    }

    pub fn merge(&mut self, is_del: bool, event: EventEntry) {
        if is_del {
            if event.has_flag(FLAG_READ) || event.has_flag(FLAG_ACCEPT) {
                self.ev_events.remove(FLAG_READ);
                self.read = None;
            }
            if event.has_flag(FLAG_ACCEPT) {
                self.ev_events.remove(FLAG_ACCEPT);
                self.accept = None;
            }
            if event.has_flag(FLAG_WRITE) {
                self.ev_events.remove(FLAG_WRITE);
                self.write = None;
            }
        } else {
            if event.has_flag(FLAG_READ) {
                self.ev_events.insert(FLAG_READ);
                if event.read.is_some() {
                    self.read = event.read;
                }
            }
            if event.has_flag(FLAG_ACCEPT) {
                self.ev_events.insert(FLAG_ACCEPT);
                if event.accept.is_some() {
                    self.accept = event.accept;
                }
            }
            if event.has_flag(FLAG_WRITE) {
                self.ev_events.insert(FLAG_WRITE);
                if event.write.is_some() {
                    self.write = event.write;
                }
            }
        }
    }
}


impl fmt::Debug for EventEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ev_fd = {}, tick_ms = {}, tick_step = {}, ev_events = {:?}",
            self.ev_fd,
            self.tick_ms,
            self.tick_step,
            self.ev_events
        )
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
