#![allow(dead_code)]
use {Timer, EventEntry};
use sys::Selector;
use std::collections::HashMap;
use {EventFlags, FLAG_PERSIST, EventBuffer, TimerCb, AcceptCb, EventCb, EndCb};
use std::io;
use std::any::Any;
use psocket::{TcpSocket, SOCKET};

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
    pub buffer_capacity: usize,
    pub time_max_id: u32,
}

impl Default for EventLoopConfig {
    fn default() -> EventLoopConfig {
        EventLoopConfig {
            io_poll_timeout_ms: 1_000,
            notify_capacity: 4_096,
            messages_per_tick: 256,
            buffer_capacity: 65_536,
            time_max_id: u32::max_value() / 2,
        }
    }
}

/// Single threaded IO event loop.
// #[derive(Debug)]
pub struct EventLoop {
    run: bool,
    timer: Timer,
    pub selector: Selector,
    config: EventLoopConfig,
    evts: Vec<EventEntry>,
    // event_maps: HashMap<i32, EventEntry>,
}


impl EventLoop {
    pub fn new() -> io::Result<EventLoop> {
        EventLoop::configured(Default::default())
    }

    pub fn configured(config: EventLoopConfig) -> io::Result<EventLoop> {
        let timer = Timer::new(config.time_max_id);
        let selector = try!(Selector::new());
        Ok(EventLoop {
            run: true,
            timer: timer,
            selector: selector,
            config: config,
            // event_maps: HashMap::new(),
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
        // self.selector.do_select1(self, 0);
        let size = try!(Selector::do_select(self, 0)) as usize;

        let size = try!(self.selector.select(&mut self.evts, 0)) as usize;
        let evts : Vec<EventEntry> = self.evts.drain(..).collect();
        // for evt in evts {
        //     if let Some(mut ev) = self.event_maps.remove(&evt.ev_fd) {
        //         let is_over = match ev.callback(self, evt.ev_events) {
        //             RetValue::OVER => true,
        //             _ => !ev.ev_events.contains(FLAG_PERSIST),
        //         };
        //         if is_over {
        //             self.del_event(ev.ev_fd, ev.ev_events);
        //         } else {
        //             self.event_maps.insert(ev.ev_fd, ev);
        //         }
        //     }
        // }

        let is_op = self.timer_process();
        // nothing todo in this loop, we will sleep 1millis
        if size == 0 && !is_op {
            ::std::thread::sleep(::std::time::Duration::from_millis(1));
        }
        Ok(())
    }

    pub fn new_buff(&self, socket: TcpSocket) -> EventBuffer {
        EventBuffer::new(socket, self.config.buffer_capacity)
    }

    /// 添加定时器, 如果time_step为0,则添加定时器失败
    pub fn add_timer(&mut self, entry: EventEntry) -> u32 {
        self.timer.add_timer(entry)
    }

    /// 添加定时器,  tick_step变量表示每隔多少ms调用一次该回调
    /// tick_repeat变量表示该定时器是否重复, 如果为true, 则会每tick_step ms进行调用一次, 直到回调返回RetValue::OVER, 或者被主动删除该定时器
    /// 添加定时器, 如果time_step为0,则添加定时器失败
    pub fn add_new_timer(&mut self, tick_step: u64,
                     tick_repeat: bool,
                     TimerCb: Option<TimerCb>,
                     data: Option<Box<Any>>) -> u32 {
        self.timer.add_timer(EventEntry::new_timer(tick_step, tick_repeat, TimerCb, data))
    }

    /// 删除指定的定时器id, 定时器内部实现细节为红黑树, 删除定时器的时间为O(logn), 如果存在该定时器, 则返回相关的定时器信息
    pub fn del_timer(&mut self, time_id: u32) -> Option<EventEntry> {
        self.timer.del_timer(time_id)
    }

    /// 添加定时器
    pub fn add_event(&mut self, entry: EventEntry) {
        let _ = self.selector.register(entry.ev_fd, entry.ev_events);
        // self.event_maps.insert(entry.ev_fd, entry);
    }

    /// 添加定时器
    pub fn add_register_socket(&mut self, buffer: EventBuffer, entry: EventEntry) {
        unsafe {
            let _ = self.selector.register_socket(buffer, entry);
        }
    }

    /// 添加定时器, ev_fd为socket的句柄id, ev_events为监听读, 写, 持久的信息
    pub fn add_new_event(&mut self, ev_fd: SOCKET,
                        ev_events: EventFlags,
                        event: Option<EventCb>,
                        error: Option<EndCb>,
                        data: Option<Box<Any>>) {
        self.add_event(EventEntry::new_event(ev_fd, ev_events, event, error, data))
    }

    /// 添加定时器, ev_fd为socket的句柄id, ev_events为监听读, 写, 持久的信息
    pub fn add_new_accept(&mut self, ev_fd: SOCKET,
                        ev_events: EventFlags,
                        accept: Option<AcceptCb>,
                        error: Option<EndCb>,
                        data: Option<Box<Any>>) {
        self.add_event(EventEntry::new_accept(ev_fd, ev_events, accept, error, data))
    }

    /// 删除指定socket的句柄信息
    pub fn del_event(&mut self, ev_fd: SOCKET, ev_events: EventFlags) -> Option<EventEntry> {
        let _ = self.selector.deregister(ev_fd, ev_events);
        None
        // self.event_maps.remove(&ev_fd)
    }
    
    /// 定时器的处理处理
    /// 1.取出定时器的第一个, 如果第一个大于当前时间, 则跳出循环, 如果小于等于当前时间进入2
    /// 2.调用回调函数, 如果回调返回OVER或者定时器不是循环定时器, 则删除定时器, 否则把该定时器重时添加到列表
    fn timer_process(&mut self) -> bool {
        let now = self.timer.now();
        let mut is_op = false;
        loop {
            match self.timer.tick_time(now) {
                Some(mut entry) => {
                    is_op = true;
                    let time_id = entry.time_id;
                    let is_over = match entry.TimerCb(self, time_id) {
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
