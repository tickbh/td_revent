use std::io::{Read, Write, Result};
use std::ptr;
use std::fmt;
use std::cmp;
use psocket::{self, TcpSocket};

pub struct Buffer {
    val: Vec<u8>,
    rpos: usize,
    wpos: usize,
}

impl Buffer {
    pub fn new() -> Buffer {
        Buffer {
            val: Vec::new(),
            rpos: 0,
            wpos: 0,
        }
    }

    pub fn get_data(&self) -> &Vec<u8> {
        &self.val
    }

    pub fn len(&self) -> usize {
        self.val.len()
    }

    pub fn empty(&self) -> bool {
        self.val.len() == 0
    }

    pub fn set_rpos(&mut self, rpos: usize) {
        self.rpos = rpos;
    }

    pub fn get_rpos(&self) -> usize {
        self.rpos
    }

    pub fn set_wpos(&mut self, wpos: usize) {
        self.wpos = wpos;
    }

    pub fn get_wpos(&self) -> usize {
        self.wpos
    }

    pub fn drain(&mut self, pos: usize) {
        self.rpos = self.rpos - cmp::min(self.rpos, pos);
        self.wpos = self.wpos - cmp::min(self.wpos, pos);
        let pos = cmp::min(self.val.len(), pos);
        self.val.drain(..pos);
    }

    pub fn drain_all(&mut self) {
        self.rpos = 0;
        self.wpos = 0;
        self.val.drain(..);
    }

    pub fn drain_collect(&mut self, pos: usize) -> Vec<u8> {
        self.rpos = self.rpos - cmp::min(self.rpos, pos);
        self.wpos = self.wpos - cmp::min(self.wpos, pos);
        let pos = cmp::min(self.val.len(), pos);
        self.val.drain(..pos).collect()
    }

    pub fn drain_all_collect(&mut self) -> Vec<u8> {
        self.rpos = 0;
        self.wpos = 0;
        self.val.drain(..).collect()
    }

    pub fn clear(&mut self) {
        self.val.clear();
        self.rpos = 0;
        self.wpos = 0;
    }
}

impl fmt::Debug for Buffer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "bytes ({:?})", self.val)
    }
}

impl Read for Buffer {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let left = self.val.len() - self.rpos;
        if left == 0 || buf.len() == 0 {
            return Ok(0);
        }
        let read = if left > buf.len() { buf.len() } else { left };
        unsafe {
            ptr::copy(&self.val[self.rpos], &mut buf[0], read);
        }
        self.rpos += read;
        Ok(read)
    }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.val.len() < self.wpos + buf.len() {
            self.val.resize(self.wpos + buf.len(), 0);
        }
        if buf.len() == 0 {
            return Ok(buf.len());
        }
        unsafe {
            ptr::copy(&buf[0], &mut self.val[self.wpos], buf.len());
        }
        self.wpos += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// 每个已分配的Socket在事件管理器都会分配一个该对象以维护已读队列, 
/// 在写队列, 标记是否正在请求写操作, 是否发生错误, 及Socket变量
pub struct EventBuffer {
    pub read: Buffer,
    pub write: Buffer,
    pub socket: TcpSocket,
    pub read_cache: Vec<u8>,
    pub is_in_write: bool,
    pub is_in_read: bool,
    pub error: Result<()>,
}

impl EventBuffer {
    pub fn new(socket: TcpSocket, capacity: usize) -> EventBuffer {
        let capacity = cmp::max(capacity, 1024);
        EventBuffer {
            read: Buffer::new(),
            write: Buffer::new(),
            socket: socket,
            read_cache: vec![0; capacity],
            is_in_write: false,
            is_in_read: false,
            error: Ok(()), 
        }
    }

    pub fn as_raw_socket(&self) -> psocket::SOCKET {
        self.socket.as_raw_socket()
    }

    pub fn has_read_buffer(&self) -> bool {
        !self.read.empty()
    }
}
