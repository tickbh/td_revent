#![allow(dead_code)]

use {Timer, EventEntry};
use sys::Selector;
use std::collections::HashMap;
use {EventFlags, FLAG_PERSIST, FLAG_READ, FLAG_WRITE};
use std::io;

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
    event_maps: HashMap<(u32, EventFlags), EventEntry>,
}


// static mut el : *mut EventLoop = 0 as *mut _;

impl EventLoop {
    // pub fn instance() -> &'static mut EventLoop {
    //     unsafe {
    //         if el == 0 as *mut _ {
    //             el = Box::into_raw(Box::new(EventLoop::new().unwrap()));
    //         }
    //         &mut *el
    //     }
    // }

    pub fn new() -> io::Result<EventLoop> {
        EventLoop::configured(Default::default())
    }

    pub fn configured(config: EventLoopConfig) -> io::Result<EventLoop> {
        let timer = Timer::new(config.timer_capacity);
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

    fn build_entry_key(fd : u32, flag : EventFlags) -> (u32, EventFlags) {
        if flag.contains(FLAG_READ) {
            (fd, FLAG_READ)
        } else if flag.contains(FLAG_WRITE) {
            (fd, FLAG_WRITE)
        } else {
            unreachable!("unkown event flag");
        }
    }

    fn convert_entry_to_key(entry : &EventEntry) -> (u32, EventFlags) {
        Self::build_entry_key(entry.ev_fd, entry.ev_events)
    }

    /// Spin the event loop once, with a timeout of one second, and notify the
    /// handler if any of the registered handles become ready during that
    /// time.
    pub fn run_once(&mut self) -> io::Result<()> {
        let size = try!(self.selector.select(&mut self.evts, 0)) as usize;
        let evts : Vec<EventEntry> = self.evts.drain(..).collect();
        for evt in evts {
            let key = Self::convert_entry_to_key(&evt);
            if self.event_maps.contains_key(&key) {
                let ev = self.event_maps[&key].clone();
                ev.callback(self, evt.ev_events);
                if !ev.ev_events.contains(FLAG_PERSIST) {
                    self.del_event(ev.ev_fd, ev.ev_events);
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

    pub fn add_timer(&mut self, entry: EventEntry) -> u32 {
        self.timer.add_timer(entry)
    }

    pub fn del_timer(&mut self, time_id: u32) -> Option<EventEntry> {
        self.timer.del_timer(time_id)
    }

    pub fn add_event(&mut self, entry: EventEntry) {
        let key = Self::convert_entry_to_key(&entry);
        let _ = self.selector.register(entry.ev_fd, entry.ev_events);
        self.event_maps.insert(key, entry);
    }

    pub fn del_event(&mut self, ev_fd: u32, ev_events: EventFlags) {
        let key = Self::build_entry_key(ev_fd, ev_events);
        let _ = self.selector.deregister(ev_fd, ev_events);
        self.event_maps.remove(&key);
    }

    fn timer_process(&mut self) -> bool {
        let now = self.timer.now();
        let mut is_op = false;
        loop {
            match self.timer.tick_time(now) {
                Some(entry) => {
                    is_op = true;
                    let ret = entry.callback(self, EventFlags::empty());
                    if ret == 0 && entry.ev_events.contains(FLAG_PERSIST) {
                        let _ = self.add_timer(entry);
                    }
                }
                _ => return is_op,
            }
        }
    }
}

unsafe impl Sync for EventLoop {}
