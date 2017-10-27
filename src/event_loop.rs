#![allow(dead_code)]
use {Timer, EventEntry};
use sys::Selector;
use std::collections::HashMap;
use {EventFlags, FLAG_PERSIST};
use std::io;
use std::any::Any;

pub enum RetValue {
    OK,
    CONTINUE,
    OVER,
}

/// Configure EventLoop runtime details
#[derive(Copy, Clone, Debug)]
pub struct EventLoopConfig {
    pub io_poll_timeout_ms: usize,

    // == Notifications ==
    pub notify_capacity: usize,
    pub messages_per_tick: usize,

    // == Timer ==
    pub timer_capacity: usize,
}

impl Default for EventLoopConfig {
    fn default() -> EventLoopConfig {
        EventLoopConfig {
            io_poll_timeout_ms: 1_000,
            notify_capacity: 4_096,
            messages_per_tick: 256,
            timer_capacity: 65_536,
        }
    }
}

/// Single threaded IO event loop.
// #[derive(Debug)]
pub struct EventLoop {
    run: bool,
    timer: Timer,
    selector: Selector,
    config: EventLoopConfig,
    evts: Vec<EventEntry>,
    event_maps: HashMap<i32, EventEntry>,
}


impl EventLoop {
    pub fn new() -> io::Result<EventLoop> {
        EventLoop::configured(Default::default())
    }

    pub fn configured(config: EventLoopConfig) -> io::Result<EventLoop> {
        let timer = Timer::new();
        let selector = try!(Selector::new());
        Ok(EventLoop {
            run: true,
            timer: timer,
            selector: selector,
            config: config,
            event_maps: HashMap::new(),
            evts: vec![],
        })
    }

    /// Tells the event loop to exit after it is done handling all events in the
    /// current iteration.
    pub fn shutdown(&mut self) {
        self.run = false;

    }

    /// Indicates whether the event loop is currently running. If it's not it has either
    /// stopped or is scheduled to stop on the next tick.
    pub fn is_running(&self) -> bool {
        self.run
    }

    /// Keep spinning the event loop indefinitely, and notify the handler whenever
    /// any of the registered handles are ready.
    pub fn run(&mut self) -> io::Result<()> {
        self.run = true;

        while self.run {
            // Execute ticks as long as the event loop is running
            try!(self.run_once());

        }

        Ok(())
    }

    /// Spin the event loop once, with a timeout of one second, and notify the
    /// handler if any of the registered handles become ready during that
    /// time.
    pub fn run_once(&mut self) -> io::Result<()> {
        let size = try!(self.selector.select(&mut self.evts, 0)) as usize;
        let evts : Vec<EventEntry> = self.evts.drain(..).collect();
        for evt in evts {
            if let Some(mut ev) = self.event_maps.remove(&evt.ev_fd) {
                let is_over = match ev.callback(self, evt.ev_events) {
                    RetValue::OVER => true,
                    _ => !ev.ev_events.contains(FLAG_PERSIST),
                };
                if is_over {
                    self.del_event(ev.ev_fd, ev.ev_events);
                } else {
                    self.event_maps.insert(ev.ev_fd, ev);
                }
            }
        }

        let is_op = self.timer_process();
        // nothing todo in this loop, we will sleep 1millis
        if size == 0 && !is_op {
            ::std::thread::sleep(::std::time::Duration::from_millis(1));
        }
        Ok(())
    }

    pub fn add_timer(&mut self, entry: EventEntry) -> i32 {
        self.timer.add_timer(entry)
    }

    pub fn add_new_timer(&mut self, tick_step: u64,
                     tick_repeat: bool,
                     call_back: Option<fn(ev: &mut EventLoop,
                                          fd: i32,
                                          flag: EventFlags,
                                          data: Option<&mut Box<Any>>)
                                          -> RetValue>,
                     data: Option<Box<Any>>) -> i32 {
        self.timer.add_timer(EventEntry::new_timer(tick_step, tick_repeat, call_back, data))
    }

    pub fn del_timer(&mut self, time_id: i32) -> Option<EventEntry> {
        self.timer.del_timer(time_id)
    }

    pub fn add_event(&mut self, entry: EventEntry) {
        let _ = self.selector.register(entry.ev_fd, entry.ev_events);
        self.event_maps.insert(entry.ev_fd, entry);
    }

    pub fn add_new_event(&mut self, ev_fd: i32,
                        ev_events: EventFlags,
                        call_back: Option<fn(ev: &mut EventLoop, fd: i32, flag: EventFlags, data: Option<&mut Box<Any>>)
                                                -> RetValue>,
                        data: Option<Box<Any>>) {
        self.add_event(EventEntry::new(ev_fd, ev_events, call_back, data))
    }

    pub fn del_event(&mut self, ev_fd: i32, ev_events: EventFlags) {
        let _ = self.selector.deregister(ev_fd, ev_events);
        self.event_maps.remove(&ev_fd);
    }

    fn timer_process(&mut self) -> bool {
        let now = self.timer.now();
        let mut is_op = false;
        loop {
            match self.timer.tick_time(now) {
                Some(mut entry) => {
                    is_op = true;
                    let is_over = match entry.callback(self, EventFlags::empty()) {
                        RetValue::OVER => true,
                        _ => !entry.ev_events.contains(FLAG_PERSIST),
                    };

                    if !is_over {
                        let _ = self.add_timer(entry);
                    }
                }
                _ => return is_op,
            }
        }
    }
}

unsafe impl Sync for EventLoop {}
