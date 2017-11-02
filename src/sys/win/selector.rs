use {EventEntry, EventFlags, FLAG_READ, FLAG_WRITE};
use std::mem;
use std::ptr;
use std::io;
use winapi;
use winapi::*;
use ws2_32::*;

pub struct Selector {
    write_sockets: Vec<SOCKET>,
    read_sockets: Vec<SOCKET>,
    read_sets: winapi::fd_set,
    write_sets: winapi::fd_set,

    event_maps: HashMap<i32, EventEntry>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        Ok(Selector {
            write_sockets: Vec::new(),
            read_sockets: Vec::new(),
            read_sets: unsafe { mem::zeroed() },
            write_sets: unsafe { mem::zeroed() },
        })
    }

    pub fn select(&mut self, evts: &mut Vec<EventEntry>, timeout: u32) -> io::Result<u32> {
        fn copy_sets(vec: &Vec<SOCKET>, fd_set: &mut winapi::fd_set, index: usize) -> usize {
            let mut new_index = index;
            fd_set.fd_count = 0;
            for i in index..vec.len() {
                fd_set.fd_array[fd_set.fd_count as usize] = vec[i];
                fd_set.fd_count += 1;
                new_index += 1;
                if fd_set.fd_count >= winapi::FD_SETSIZE as u32 {
                    return new_index;
                }
            }
            new_index
        }

        evts.clear();
        let mut size = 0;
        let mut read_index = 0;
        let mut write_index = 0;
        let mut time = timeval {
            tv_sec: (timeout / 1000) as i32,
            tv_usec: ((timeout % 1000) * 1000) as i32,
        };
        while read_index < self.read_sockets.len() || write_index < self.write_sockets.len() {
            read_index += copy_sets(&self.read_sockets, &mut self.read_sets, read_index);
            write_index += copy_sets(&self.write_sockets, &mut self.write_sets, write_index);
            let count = unsafe {
                select(0,
                       &mut self.read_sets,
                       &mut self.write_sets,
                       ptr::null_mut(),
                       &mut time)
            };
            if count <= 0 {
                continue;
            }

            if self.read_sets.fd_count > 0 {
                for i in 0..self.read_sets.fd_count {
                    evts.push(EventEntry::new_evfd(self.read_sets.fd_array[i as usize] as i32,
                                                   FLAG_READ));
                }
                size += self.read_sets.fd_count;
            }
            if self.write_sets.fd_count > 0 {
                for i in 0..self.write_sets.fd_count {
                    evts.push(EventEntry::new_evfd(self.write_sets.fd_array[i as usize] as i32,
                                                   FLAG_WRITE));
                }
                size += self.write_sets.fd_count;
            }
        }
        Ok(size)
    }

    pub fn register(&mut self, fd: i32, ev_events: EventFlags) {
        let fd = fd as SOCKET;
        if ev_events.contains(FLAG_READ) && !self.read_sockets.contains(&fd) {
            self.read_sockets.push(fd);
        }
        if ev_events.contains(FLAG_WRITE) && !self.write_sockets.contains(&fd) {
            self.write_sockets.push(fd);
        }
    }

    pub fn deregister(&mut self, fd: i32, flag: EventFlags) {
        let fd = fd as SOCKET;
        fn search_index(vec: &Vec<SOCKET>, value: &SOCKET) -> Option<usize> {
            for (i, v) in vec.iter().enumerate() {
                if value == v {
                    return Some(i);
                }
            }
            None
        };

        if flag.contains(FLAG_READ) {
            if let Some(index) = search_index(&self.read_sockets, &fd) {
                self.read_sockets.remove(index);
            }
        }

        if flag.contains(FLAG_WRITE) {
            if let Some(index) = search_index(&self.write_sockets, &fd) {
                self.write_sockets.remove(index);
            }
        }
    }
}
