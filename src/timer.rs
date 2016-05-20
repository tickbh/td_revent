use EventEntry;
use std::fmt;
use std::cmp;
use std::collections::BinaryHeap;
use std::collections::HashSet;
extern crate libc;
extern crate time;


pub struct Timer {
    timer_queue: BinaryHeap<EventEntry>,
    time_sets: HashSet<u32>,
    time_id: u32,
}

impl Timer {
    pub fn new(capacity: usize) -> Timer {
        Timer {
            timer_queue: BinaryHeap::with_capacity(capacity),
            time_sets: HashSet::new(),
            time_id: 0,
        }
    }

    pub fn now(&self) -> u64 {
        time::precise_time_ns() / 1000
    }

    // ID = 0 为无效ID
    pub fn add_timer(&mut self, mut entry: EventEntry) -> u32 {
        if entry.ev_fd == 0 {
            entry.ev_fd = self.calc_new_id();
        };
        let time_id = entry.ev_fd;
        entry.tick_ms = self.now() + entry.tick_step;
        self.timer_queue.push(entry);
        time_id
    }

    pub fn del_timer(&mut self, time_id: u32) -> Option<EventEntry> {
        let mut ret: Option<EventEntry> = None;
        let mut data = Vec::new();
        while let Some(entry) = self.timer_queue.pop() {
            if entry.ev_fd != time_id {
                data.push(entry);
            } else {
                ret = Some(entry);
            }
        }
        self.time_sets.remove(&time_id);
        self.timer_queue = BinaryHeap::from(data);
        ret
    }

    pub fn tick_first(&self) -> Option<u64> {
        self.timer_queue
            .peek()
            .map(|entry| Some(entry.tick_ms))
            .unwrap_or(None)
    }


    pub fn tick_time(&mut self, tm: u64) -> Option<EventEntry> {
        if tm < self.tick_first().unwrap_or(tm + 1) {
            return None;
        }
        self.timer_queue.pop()
    }

    fn calc_new_id(&mut self) -> u32 {
        loop {
            self.time_id = self.time_id.overflowing_add(1).0;
            self.time_id = cmp::max(self.time_id, 1);
            if self.time_sets.contains(&self.time_id) {
                continue;
            }
            self.time_sets.insert(self.time_id);
            break;
        }
        self.time_id
    }
}

impl fmt::Debug for Timer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for entry in &(self.timer_queue) {
            let _ = writeln!(f, "{:?}", entry);
        }
        write!(f, "")
    }
}
