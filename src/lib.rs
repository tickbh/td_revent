#[macro_use]
extern crate bitflags;
extern crate libc;
#[cfg(windows)]
extern crate winapi;
#[cfg(windows)]
extern crate ws2_32;

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
pub use sys::AsFd;