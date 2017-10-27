use EventEntry;
use std::fmt;
use std::cmp::{self, Ord, Ordering};
use std::collections::HashMap;
use rbtree::RBTree;

extern crate libc;
extern crate time;

pub struct Timer {
    timer_queue: RBTree<TreeKey, EventEntry>,
    time_maps: HashMap<i32, u64>,
    time_id: i32,
}

#[derive(PartialEq, Eq)]
struct TreeKey(u64, i32);

impl Ord for TreeKey {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0 != other.0 {
            return self.0.cmp(&other.0);
        }
        other.1.cmp(&self.1)
    }
}

impl PartialOrd for TreeKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}


impl Timer {
    pub fn new() -> Timer {
        Timer {
            timer_queue: RBTree::new(),
            time_maps: HashMap::new(),
            time_id: 0,
        }
    }

    pub fn now(&self) -> u64 {
        time::precise_time_ns() / 1000
    }

    // ID = 0 为无效ID
    pub fn add_timer(&mut self, mut entry: EventEntry) -> i32 {
        if entry.ev_fd == 0 {
            entry.ev_fd = self.calc_new_id();
        };
        let time_id = entry.ev_fd;
        entry.tick_ms = self.now() + entry.tick_step;
        self.time_maps.insert(time_id, entry.tick_ms);
        self.timer_queue.insert(TreeKey(entry.tick_ms, time_id), entry);
        time_id
    }

    pub fn del_timer(&mut self, time_id: i32) -> Option<EventEntry> {
        if !self.time_maps.contains_key(&time_id) {
            return None;
        }
        let key = TreeKey(self.time_maps[&time_id], time_id);
        self.time_maps.remove(&time_id);
        self.timer_queue.remove(&key)
    }

    pub fn tick_first(&self) -> Option<u64> {
        self.timer_queue
            .get_first()
            .map(|(key, _)| Some(key.0))
            .unwrap_or(None)
    }

    pub fn tick_time(&mut self, tm: u64) -> Option<EventEntry> {
        if tm < self.tick_first().unwrap_or(tm + 1) {
            return None;
        }
        if let Some((key, entry)) = self.timer_queue.pop_first() {
            self.time_maps.remove(&key.1);
            Some(entry)
        } else {
            None
        }
    }

    fn calc_new_id(&mut self) -> i32 {
        loop {
            self.time_id = self.time_id.overflowing_add(1).0;
            self.time_id = cmp::max(self.time_id, 1);
            if self.time_maps.contains_key(&self.time_id) {
                continue;
            }
            break;
        }
        self.time_id
    }
}

impl fmt::Debug for Timer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (_, entry) in self.timer_queue.iter() {
            let _ = writeln!(f, "{:?}", entry);
        }
        write!(f, "")
    }
}
