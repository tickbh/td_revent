use {EventEntry, EventFlags, EventBuffer, EventLoop, RetValue};
use std::collections::HashMap;
use std::mem;
use psocket::SOCKET;
use std::cell::UnsafeCell;
use std::io::{self, ErrorKind};
use std::time::Duration;
use sys::win::iocp::{CompletionPort, CompletionStatus};
use sys::win::{FromRawArc, Overlapped};
use super::{TcpSocketExt, AcceptAddrsBuf};
use psocket::{TcpSocket, SocketAddr};
use winapi::*;
use std::io::prelude::*;


macro_rules! overlapped2arc {
    ($e:expr, $t:ty, $($field:ident).+) => ({
        unsafe {
            let offset = offset_of!($t, $($field).+);
            debug_assert!(offset < mem::size_of::<$t>());
            FromRawArc::from_raw(($e as usize - offset) as *mut $t)
        }
    })
}

macro_rules! offset_of {
    ($t:ty, $($field:ident).+) => (
        &(*(0 as *const $t)).$($field).+ as *const _ as usize
    )
}

pub struct Event {
    pub buffer: EventBuffer,
    pub entry: EventEntry,
    pub read: CbOverlapped,
    pub write: CbOverlapped,
    pub accept_buf: Option<AcceptAddrsBuf>,
    pub accept_socket: Option<TcpSocket>,
    pub is_end: bool,
}

impl Event {
    pub fn is_accept(&self) -> bool {
        self.entry.ev_events.contains(EventFlags::FLAG_ACCEPT)
    }

    pub fn as_raw_socket(&self) -> SOCKET {
        self.buffer.socket.as_raw_socket()
    }

    pub fn new(buffer: EventBuffer, entry: EventEntry) -> Event {
        Event {
            buffer: buffer,
            entry: entry,
            read: CbOverlapped::new(read_done),
            write: CbOverlapped::new(write_done),
            accept_socket: None,
            accept_buf: Some(AcceptAddrsBuf::new()),
            is_end: false,
        }
    }
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

#[derive(Debug)]
pub struct Events {
    /// Raw I/O event completions are filled in here by the call to `get_many`
    /// on the completion port above. These are then processed to run callbacks
    /// which figure out what to do after the event is done.
    statuses: Box<[CompletionStatus]>,
}

#[repr(C)]
pub struct CbOverlapped {
    inner: UnsafeCell<Overlapped>,
    callback: fn(&mut EventLoop, &OVERLAPPED_ENTRY),
}


impl CbOverlapped {
    /// Creates a new `Overlapped` which will invoke the provided `cb` callback
    /// whenever it's triggered.
    ///
    /// The returned `Overlapped` must be used as the `OVERLAPPED` passed to all
    /// I/O operations that are registered with mio's event loop. When the I/O
    /// operation associated with an `OVERLAPPED` pointer completes the event
    /// loop will invoke the function pointer provided by `cb`.
    pub fn new(cb: fn(&mut EventLoop, &OVERLAPPED_ENTRY)) -> CbOverlapped {
        CbOverlapped {
            inner: UnsafeCell::new(Overlapped::zero()),
            callback: cb,
        }
    }

    /// Get the underlying `Overlapped` instance as a raw pointer.
    ///
    /// This can be useful when only a shared borrow is held and the overlapped
    /// pointer needs to be passed down to winapi.
    pub fn as_mut_ptr(&self) -> *mut OVERLAPPED {
        unsafe { (*self.inner.get()).raw() }
    }
}

impl Events {
    pub fn with_capacity(cap: usize) -> Events {
        Events {
            statuses: vec![CompletionStatus::zero(); cap].into_boxed_slice(),
        }
    }
}

fn read_done(event_loop: &mut EventLoop, status: &OVERLAPPED_ENTRY) {
    let status = CompletionStatus::from_entry(status);
    let mut event = overlapped2arc!(status.overlapped(), Event, read);
    if event.is_accept() {
        let mut socket = event.accept_socket.take().unwrap();
        let result = event.buffer.socket.accept_complete(&socket).and_then(|()| {
            event.accept_buf.as_ref().unwrap().parse(&event.buffer.socket)
        }).and_then(|buf| {
            buf.remote().ok_or_else(|| {
                io::Error::new(ErrorKind::Other, "could not obtain remote address")
            })
        });
        event.buffer.is_in_read = false;
        let ret = match result {
            Ok(remote_addr) => {
                socket.set_peer_addr(remote_addr);
                event.entry.accept_cb(event_loop, Ok(socket))
            }
            Err(e) => event.entry.accept_cb(event_loop, Err(e)),
        };
        match ret {
            RetValue::OVER => {
                let _ = event_loop.unregister_socket(event.as_raw_socket());
            }
            _ => {
                
                if !event.entry.has_flag(EventFlags::FLAG_PERSIST) && !event.entry.has_flag(EventFlags::FLAG_READ_PERSIST) {
                    event.entry.ev_events.remove(EventFlags::FLAG_READ);
                    event.entry.ev_events.remove(EventFlags::FLAG_ACCEPT);
                }
                
                if !event.entry.has_flag(EventFlags::FLAG_ACCEPT) {
                    return;
                }
                
                if let Err(err) = event_loop.selector.post_accept_event(
                    event.as_raw_socket(),
                )
                {
                    event.buffer.error = Err(err);
                    let _ = event_loop.unregister_socket(event.as_raw_socket());
                } else {
                    event.buffer.is_in_read = true;
                }
            }
        }
        return;
    } else {
        let bytes_transferred = status.bytes_transferred() as usize;
        if status.flag().contains(EventFlags::FLAG_ENDED) {
            let _ = Selector::_unregister_socket(
                event_loop,
                event.buffer.as_raw_socket()
            );
            return;
        }

        if bytes_transferred == 0 {
            let _ = Selector::unregister_socket(
                event_loop,
                event.buffer.as_raw_socket()
            );
            return;
        }
        let mut event_clone = event.clone();
        if bytes_transferred > 0 {
            let _ = event.buffer.read.write(
                &event_clone.buffer.read_cache
                    [..bytes_transferred],
            );
        }
        event.buffer.is_in_read = false;
        if event.buffer.has_read_buffer() {
            match event.entry.read_cb(event_loop, &mut event_clone.buffer) {
                RetValue::OVER => {
                    let _ = event_loop.unregister_socket(event.as_raw_socket());
                    return;
                }
                _ => (),
            }
        }

        if !event.entry.has_flag(EventFlags::FLAG_PERSIST) && !event.entry.has_flag(EventFlags::FLAG_READ_PERSIST) {
            event.entry.ev_events.remove(EventFlags::FLAG_READ);
        }

        if !event.entry.has_flag(EventFlags::FLAG_READ) {
            return;
        }

        match event_loop.selector.post_read_event(
            &event.as_raw_socket(),
        ) {
            Err(e) => {
                event.buffer.error = Err(e);
                let _ = event_loop.unregister_socket(event.as_raw_socket());
            },
            _ => {
                event.buffer.is_in_read = true;
            }
        }
    }
}

fn write_done(event_loop: &mut EventLoop, status: &OVERLAPPED_ENTRY) {
    let status = CompletionStatus::from_entry(status);
    let mut event = overlapped2arc!(status.overlapped(), Event, write);
    let mut event_clone = event.clone();
    event.buffer.is_in_write = false;

    match event.entry.write_cb(event_loop, &mut event_clone.buffer) {
        RetValue::OVER => {
            let _ = event_loop.unregister_socket(event.as_raw_socket());
            return;
        }
        _ => (),
    }

    if !event.buffer.write.empty() {
        let _ = event_loop.selector.post_write_event(
            &event.buffer.as_raw_socket(),
            None,
        );
    }
}

pub struct Selector {
    port: CompletionPort,
    events: Events,
    event_maps: HashMap<SOCKET, EventImpl>,
}

impl Selector {
    pub fn new(capacity: usize) -> io::Result<Selector> {
        Ok(Selector {
            port: CompletionPort::new(1)?,
            events: Events::with_capacity(capacity),
            event_maps: HashMap::new(),
        })
    }

    /// 获取当前可执行的事件, 并同时处理数据, 返回执行的个数
    pub fn do_select(event: &mut EventLoop, timeout: usize) -> io::Result<usize> {
        let n = match event.selector.port.get_many(
            &mut event.selector.events.statuses,
            Some(Duration::from_millis(timeout as u64)),
        ) {
            Ok(statuses) => statuses.len(),
            Err(ref e) if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) => 0,
            Err(e) => return Err(e),
        };

        let statuses = event.selector.events.statuses[..n].to_vec();
        for status in statuses {
            // This should only ever happen from the awakener, and we should
            // only ever have one awakener right now, so assert as such.
            if status.overlapped() as usize == 0 {
                continue;
            }

            let callback = unsafe { (*(status.overlapped() as *mut CbOverlapped)).callback };
            callback(event, status.entry());
        }
        Ok(n)
    }

    /// 向iocp投递接受socket事件, 只有listener的socket投递该事件才有效
    /// 接受事件会预先准备好socket, 等待iocp的回调, 回调成功会再次进行该投递, 保证一直可接受新的
    fn post_accept_event(&mut self, socket: SOCKET) -> io::Result<()> {
        if let Some(event) = self.event_maps.get_mut(&socket) {
            let event = &mut (*event.inner);
            if event.buffer.is_in_read {
                return Ok(());
            }
            let addr = event.buffer.socket.local_addr()?;
            event.accept_socket = Some(match addr {
                SocketAddr::V4(..) => TcpSocket::new_v4()?,
                SocketAddr::V6(..) => TcpSocket::new_v6()?,
            });
            unsafe {
                event.buffer.socket.accept_overlapped(
                    &event.accept_socket.as_ref().unwrap(),
                    event.accept_buf.as_mut().unwrap(),
                    event.read.as_mut_ptr(),
                )?;
            }
        }
        Ok(())
    }

    /// 向iocp投递读的事件, 每个socket确保只有一个读事件正在执行, 确保数据不会被打乱
    /// 投递完成事件只有在初始添加时添加, 和读回调后再进行投递
    /// 每次投递不管是立即返回还是WSA_IO_PENDING, 都将会在GetQueuedCompletionStatus得到结果
    /// 所以在此处不做数据处理, 仅仅只进行数据投递, 在回调的时候根据bytes_transferred获取读的数量
    fn post_read_event(&mut self, socket: &SOCKET) -> io::Result<()> {
        if let Some(ev) = self.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if event.is_end {
                return Ok(());
            }
            if event.buffer.is_in_read {
                return Ok(());
            }
            unsafe {
                event.buffer.socket.read_overlapped(
                    &mut event.buffer.read_cache[..],
                    event.read.as_mut_ptr(),
                )?
            };
        }
        Ok(())
    }

    /// 向iocp投递写的事件, 如果正在写入, 或者写缓存为空, 或者已结束就不投递事件
    /// 用WSASend发送相关的消息, 可立即得到发送的字节数, 清除相关的写缓存, 并返回大小
    /// 如果写缓存数据没有全部被写入, 则表示当前无法全部写入, 设置socket状态在写状态
    /// 等待写完成的事件通知, 再写入剩余的相关数据
    fn post_write_event(&mut self, socket: &SOCKET, data: Option<&[u8]>) -> io::Result<usize> {
        if let Some(ev) = self.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if data.is_some() {
                event.buffer.write.write(data.unwrap())?;
            }
            if event.buffer.is_in_write || event.buffer.write.empty() || event.is_end {
                return Ok(0);
            }

            let write = event.write.as_mut_ptr();
            let res = unsafe {
                event.buffer.socket.write_overlapped(
                    &event.buffer.write.get_data()[..],
                    write,
                )?
            };

            match res {
                Some(n) => {
                    event.buffer.write.drain(n);
                    //如果写入包没有写入完毕, 则表示iocp已被填满, 如果下次写入的将等待write事件返回再次写入
                    if !event.buffer.write.empty() {
                        event.buffer.is_in_write = true;
                    }
                    return Ok(n);
                }
                _ => {
                    return Ok(0);
                }
            }
        }
        Err(io::Error::new(
            ErrorKind::Other,
            "the socket already be remove",
        ))
    }

    fn check_socket_event(&mut self, socket: SOCKET) -> io::Result<()> {
        if !self.event_maps.contains_key(&socket) {
            return Ok(());
        }

        let flag = {
            let event = &self.event_maps[&socket];
            (event.inner).entry.ev_events
        };

        if flag.contains(EventFlags::FLAG_ACCEPT) {
            self.post_accept_event(socket)?;
        } else {
            if flag.contains(EventFlags::FLAG_READ) {
                self.post_read_event(&socket)?;
            }
            if flag.contains(EventFlags::FLAG_WRITE) {
                self.post_write_event(&socket, None)?;
            }
        }
        Ok(())
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

        selector.port.add_socket(entry.ev_events, &buffer.socket)?;
        let event = Event::new(buffer, entry);
        selector.event_maps.insert(socket, EventImpl::new(event));
        if let Err(e) = selector.check_socket_event(socket) {
            selector.event_maps.remove(&socket);
            return Err(e);
        }
        Ok(())
    }

    /// 修改socket事件, 把socket投递到iocp的监听中
    pub fn modify_socket(
        event_loop: &mut EventLoop,
        is_del: bool,
        socket: SOCKET,
        entry: EventEntry,
    ) -> io::Result<()> {
        let err = {
            let selector = &mut event_loop.selector;
            if !selector.event_maps.contains_key(&socket) {
                return Ok(())
            }

            if let Some(ev) = selector.event_maps.get_mut(&socket) {
                let event = &mut (*ev.clone().inner);
                event.entry.merge(is_del, entry);
            }

            if let Err(e) = selector.check_socket_event(socket) {
                Err(e)
            } else {
                return Ok(())
            }
        };
        Self::unregister_socket(event_loop, socket)?;
        return err;
    }

    /// 收到EventFlags::FLAG_ENDED事件的时候, 把相关的socket资源全部释放完毕
    /// 并触发end_cb事件, 如果有关注此事件, 可得到当前socket的最后状态
    fn _unregister_socket(
        event_loop: &mut EventLoop,
        socket: SOCKET,
    ) -> io::Result<()> {
        if let Some(mut ev) = event_loop.selector.event_maps.remove(&socket) {
            let event = &mut (*ev.clone().inner);
            let event_clone = &mut (*ev.inner);
            event.entry.end_cb(event_loop, &mut event_clone.buffer);
        }
        Ok(())
    }

    /// 取消某个socket的监听, iocp模式下flags参数无效
    /// iocp模式下, 会把事件置成已完成状态, 这时不可写不可读
    /// 并且把指定的socket手动关闭保证iocp里面的read和write事件先被唤醒
    /// 然后发送EventFlags::FLAG_ENDED事件, 进行最终析构, 确保资源正确的释放
    pub fn unregister_socket(
        event_loop: &mut EventLoop,
        socket: SOCKET,
    ) -> io::Result<()> {
        if let Some(ev) = event_loop.selector.event_maps.get_mut(&socket) {
            let event = &mut (*ev.clone().inner);
            if event.is_end {
                return Ok(());
            }
            event.buffer.socket.close();
            event.is_end = true;
            event_loop.selector.port.post_info(0, EventFlags::FLAG_ENDED, event.read.as_mut_ptr())?;
        }
        Ok(())
    }

    // 给指定的socket发送数据, 如果不能一次发送完毕则会写入到缓存中, 等待下次继续发送
    // 返回值为指定的当次的写入大小, 如果没有全部写完数据, 则下次写入先写到缓冲中, 等待系统的可写通知
    pub fn send_socket(event_loop: &mut EventLoop, socket: &SOCKET, data: &[u8]) -> io::Result<usize> {
        event_loop.selector.post_write_event(socket, Some(data))
    }
}
