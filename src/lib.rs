#![feature(collections)]
#![feature(const_fn)]
#![feature(box_raw)]
#![feature(negate_unsigned)]
#![feature(box_syntax)]
#![feature(rt)]

#[macro_use]
extern crate bitflags;

extern crate libc;

mod event_loop;
mod timer;
mod event_flags;
mod event_entry;

pub use timer::{Timer};
pub use event_loop::{
    EventLoop,
    EventLoopConfig,
};

pub mod sys;

pub use event_flags::{EventFlags, FLAG_TIMEOUT, FLAG_READ, FLAG_WRITE, FLAG_PERSIST};
pub use event_entry::EventEntry;