#![allow(dead_code)]

use {Timer, EventEntry};
use sys::Selector;
use std::collections::HashMap;
use {EventFlags, FLAG_PERSIST};
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
    selector : Selector,
    config: EventLoopConfig,
    evts : Vec<EventEntry>, 
    event_maps : HashMap<u64, EventEntry>,
}


static mut el : *mut EventLoop = 0 as *mut _;

impl EventLoop {


    pub fn instance() -> &'static mut EventLoop {
        unsafe {
            if el == 0 as *mut _ {
                el = Box::into_raw(Box::new(EventLoop::new().unwrap()));
            }
            &mut *el
        }
    }

    pub fn new() -> io::Result<EventLoop> {
        EventLoop::configured(Default::default())
    }

    pub fn configured(config: EventLoopConfig) -> io::Result<EventLoop> {
        let timer = Timer::new(config.timer_capacity);
        let selector = try!(Selector::new());
        Ok(EventLoop {
            run: true,
            timer: timer,
            selector : selector,
            config: config,
            event_maps : HashMap::new(),
            evts : vec![],
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
        for index in 0 .. size {
            let evt = self.evts[index].clone();
            if self.event_maps.contains_key(&evt.ev_fd) {
                let ev = self.event_maps[&evt.ev_fd].clone();
                ev.callback(self, evt.ev_events);
            }
        }
        self.timer_process();
        Ok(())
    }

    pub fn add_timer(&mut self, entry : EventEntry) -> u64 {
        self.timer.add_timer(entry)
    }

    pub fn del_timer(&mut self, time_id : u64) {
        self.timer.del_timer(time_id);
    }

    pub fn add_event(&mut self, entry : EventEntry) {
        println!("add event {:?}", entry);
        let _ = self.selector.register(entry.ev_fd, entry.ev_events);
        self.event_maps.insert(entry.ev_fd, entry);
    }

    pub fn del_event(&mut self, ev_fd : u64, ev_events : EventFlags) {
        let _ = self.selector.deregister(ev_fd, ev_events);
        self.event_maps.remove(&ev_fd);
    }

    fn timer_process(&mut self) {
        let now = self.timer.now();
        loop {
            match self.timer.tick_time(now) {
                Some(entry) => {
                    let ret = entry.callback(self, EventFlags::empty());
                    if ret == 0 && entry.ev_events.contains(FLAG_PERSIST)  {
                        let _ = self.add_timer(entry);
                    }
                },
                _ => return
            }
        }
    }
}

unsafe impl Sync for EventLoop {
    
}
