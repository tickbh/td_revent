#![allow(dead_code)]
use {Timer, EventEntry};
use sys::Selector;
use {EventFlags, FLAG_PERSIST, EventBuffer, TimerCb, AcceptCb, EventCb, EndCb};
use std::io;
use std::any::Any;
use psocket::{TcpSocket, SOCKET};

///回调的函数返回值, 如果返回OK和CONTINUE, 则默认处理
///如果返回OVER则主动结束循环, 比如READ则停止READ, 定时器如果是循环的则主动停止当前的定时器 
pub enum RetValue {
    OK,
    CONTINUE,
    OVER,
}

/// Configure EventLoop runtime details
#[derive(Copy, Clone, Debug)]
pub struct EventLoopConfig {
    pub io_poll_timeout_ms: usize,

    pub select_catacity: usize,
    pub buffer_capacity: usize,

    // == Timer ==
    pub time_max_id: u32,
}

impl Default for EventLoopConfig {
    fn default() -> EventLoopConfig {
        EventLoopConfig {
            io_poll_timeout_ms: 1,

            select_catacity: 1024,
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
}


impl EventLoop {
    pub fn new() -> io::Result<EventLoop> {
        EventLoop::configured(Default::default())
    }

    pub fn configured(config: EventLoopConfig) -> io::Result<EventLoop> {
        let timer = Timer::new(config.time_max_id);
        let selector = try!(Selector::new(config.select_catacity));
        Ok(EventLoop {
            run: true,
            timer: timer,
            selector: selector,
            config: config,
        })
    }

    /// 关闭主循环, 将在下一次逻辑执行时退出主循环
    pub fn shutdown(&mut self) {
        self.run = false;

    }

    /// 判断刚才主循环是否在运行中
    pub fn is_running(&self) -> bool {
        self.run
    }

    /// 循环执行事件的主逻辑, 直到此主循环被shutdown则停止执行
    pub fn run(&mut self) -> io::Result<()> {
        self.run = true;

        while self.run {
            // 该此循环中, 没有任何数据得到处理, 则强制cpu休眠1ms, 以防止cpu跑满100%
            if !self.run_once()? {
                ::std::thread::sleep(::std::time::Duration::from_millis(1));
            }

        }
        Ok(())
    }

    /// 进行一次的数据处理, 处理包括处理sockets信息, 及处理定时器的信息
    pub fn run_once(&mut self) -> io::Result<bool> {
        let timeout_ms = self.config.io_poll_timeout_ms;
        let size = Selector::do_select(self, timeout_ms)?;
        let is_op = self.timer_process();
        Ok(size != 0 || !is_op)
    }

    /// 根据socket构造EventBuffer
    pub fn new_buff(&self, socket: TcpSocket) -> EventBuffer {
        EventBuffer::new(socket, self.config.buffer_capacity)
    }

    /// 添加定时器, 如果time_step为0, 则添加定时器失败
    pub fn add_timer(&mut self, entry: EventEntry) -> u32 {
        self.timer.add_timer(entry)
    }

    /// 添加定时器,  tick_step变量表示每隔多少ms调用一次该回调
    /// tick_repeat变量表示该定时器是否重复, 如果为true, 则会每tick_step ms进行调用一次, 直到回调返回RetValue::OVER, 或者被主动删除该定时器
    /// 添加定时器, 如果time_step为0, 则添加定时器失败
    pub fn add_new_timer(
        &mut self,
        tick_step: u64,
        tick_repeat: bool,
        timer_cb: Option<TimerCb>,
        data: Option<Box<Any>>,
    ) -> u32 {
        self.timer.add_first_timer(EventEntry::new_timer(
            tick_step,
            tick_repeat,
            timer_cb,
            data,
        ))
    }

    /// 添加定时器,  tick_time指定某一时间添加触发定时器
    pub fn add_new_timer_at(
        &mut self,
        tick_time: u64,
        timer_cb: Option<TimerCb>,
        data: Option<Box<Any>>,
    ) -> u32 {
        self.timer.add_first_timer(EventEntry::new_timer_at(
            tick_time,
            timer_cb,
            data,
        ))
    }

    /// 删除指定的定时器id, 定时器内部实现细节为红黑树, 删除定时器的时间为O(logn), 如果存在该定时器, 则返回相关的定时器信息
    pub fn del_timer(&mut self, time_id: u32) -> Option<EventEntry> {
        self.timer.del_timer(time_id)
    }

    /// 添加socket监听
    pub fn register_socket(&mut self, buffer: EventBuffer, entry: EventEntry) -> io::Result<()> {
        let _ = Selector::register_socket(self, buffer, entry)?;
        Ok(())
    }

    /// 删除指定socket的句柄信息
    pub fn unregister_socket(&mut self, ev_fd: SOCKET, ev_events: EventFlags) -> io::Result<()> {
        let _ = Selector::unregister_socket(self, ev_fd, ev_events)?;
        Ok(())
    }

    /// 向指定socket发送数据, 返回发送的数据长度
    pub fn send_socket(&mut self, ev_fd: &SOCKET, data: &[u8]) -> io::Result<usize> {
        Selector::send_socket(self, ev_fd, data)
    }

    /// 添加定时器, ev_fd为socket的句柄id, ev_events为监听读, 写, 持久的信息
    pub fn add_new_event(
        &mut self,
        socket: TcpSocket,
        ev_events: EventFlags,
        read: Option<EventCb>,
        write: Option<EventCb>,
        error: Option<EndCb>,
        data: Option<Box<Any>>,
    ) -> io::Result<()> {
        let ev_fd = socket.as_raw_socket();
        let buffer = self.new_buff(socket);
        self.register_socket(buffer, EventEntry::new_event(ev_fd, ev_events, read, write, error, data))
    }

    /// 添加定时器, ev_fd为socket的句柄id, ev_events为监听读, 写, 持久的信息
    pub fn add_new_accept(
        &mut self,
        socket: TcpSocket,
        ev_events: EventFlags,
        accept: Option<AcceptCb>,
        error: Option<EndCb>,
        data: Option<Box<Any>>,
    ) -> io::Result<()> {
        let ev_fd = socket.as_raw_socket();
        let buffer = self.new_buff(socket);
        self.register_socket(buffer, EventEntry::new_accept(ev_fd, ev_events, accept, error, data))
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
                    let is_over = match entry.timer_cb(self, time_id) {
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
