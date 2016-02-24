use {EventEntry, EventFlags, FLAG_READ, FLAG_WRITE};
use super::winsock;
use std::mem;
use std::ptr;
use std::io;

extern crate libc;

pub struct Selector {
    write_sockets : Vec<libc::SOCKET>,
    read_sockets : Vec<libc::SOCKET>,
    read_sets : winsock::fd_set,
    write_sets : winsock::fd_set,
}

impl Selector {

	pub fn new() -> io::Result<Selector> {
		Ok(Selector {
            write_sockets : Vec::new(),
            read_sockets : Vec::new(),
            read_sets : unsafe { mem::zeroed() },
            write_sets : unsafe { mem::zeroed() },
        })
	}

    pub fn select(&mut self, evts : &mut Vec<EventEntry>, timeout : u32) -> io::Result<u32> {
        fn copy_sets(vec : &Vec<libc::SOCKET>, fd_set : &mut winsock::fd_set, index : usize) -> usize {
            let new_index = index;
            fd_set.fd_count = 0;
            for i in index .. vec.len() {
                winsock::fd_set(fd_set, vec[i]);
                if fd_set.fd_count >= winsock::FD_SETSIZE as u32 {
                    return new_index;
                }
            }
            vec.len()
        }

        evts.clear();
        let mut size = 0;
        let mut read_index = 0;
        let mut write_index = 0;
        let mut time = libc::timeval {
            tv_sec : (timeout / 1000) as i32,
            tv_usec : ((timeout % 1000) * 1000) as i32,
        };

        while read_index < self.read_sockets.len() || write_index < self.write_sockets.len() {
            read_index = copy_sets(&self.read_sockets, &mut self.read_sets, read_index);
            write_index = copy_sets(&self.write_sockets, &mut self.write_sets, write_index);
            let count = unsafe { winsock::select(0, &mut self.read_sets, &mut self.write_sets, ptr::null_mut(), &mut time) };
            if count <= 0 {
                continue;
            }
                        println!("count is {}", count);

            if self.read_sets.fd_count > 0 {
                for i in 0 .. self.read_sets.fd_count {
                    evts.push(EventEntry::new_evfd(self.read_sets.fd_array[i as usize] as u64, FLAG_READ));
                }
                size += self.read_sets.fd_count;
            }
            if self.write_sets.fd_count > 0 {
                for i in 0 .. self.write_sets.fd_count {
                    evts.push(EventEntry::new_evfd(self.write_sets.fd_array[i as usize] as u64, FLAG_WRITE));
                }
                size += self.write_sets.fd_count;
            }
        };
        Ok(size)
    }

    pub fn register(&mut self, fd : u64, ev_events : EventFlags) {
        let fd = fd as libc::SOCKET;
        if ev_events.contains(FLAG_READ) && !self.read_sockets.contains(&fd) {
            self.read_sockets.push(fd);
        }
        if ev_events.contains(FLAG_WRITE) && !self.read_sockets.contains(&fd) {
            self.write_sockets.push(fd);
        }
    }

    pub fn deregister(&mut self, fd : u64, _ : EventFlags) {
        let fd = fd as libc::SOCKET;
        fn search_index(vec : &Vec<libc::SOCKET>, value : &libc::SOCKET) -> Option<usize> {
            for i in 0 .. vec.len() {
                if *value == vec[i] {
                    return Some(i);
                }
            }
            None
        };

        if let Some(index) = search_index(&self.read_sockets, &fd) {
            self.read_sockets.remove(index);
        }

        if let Some(index) = search_index(&self.write_sockets, &fd) {
            self.write_sockets.remove(index);
        }
    }
}