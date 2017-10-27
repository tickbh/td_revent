#[macro_use]
extern crate bitflags;
extern crate libc;
#[cfg(windows)]
extern crate winapi;
#[cfg(windows)]
extern crate ws2_32;

extern crate nix;

extern crate rbtree;
extern crate psocket;

mod event_loop;
mod timer;
mod event_flags;
mod event_entry;

pub use timer::Timer;
pub use event_loop::{EventLoop, EventLoopConfig, RetValue};

pub mod sys;

pub use event_flags::{EventFlags, FLAG_TIMEOUT, FLAG_READ, FLAG_WRITE, FLAG_PERSIST, FLAG_ERROR};
pub use event_entry::EventEntry;
pub use sys::{AsFd, FromFd};

/// The macro convert Option<&mut Box<Any>> to &mut ty
#[macro_export]
macro_rules! any_to_mut {
    ( $x:expr, $t:ty ) => {
        $x.unwrap().downcast_mut::<$t>().unwrap()
    };
}

/// The macro convert Option<&mut Box<Any>> to &ty
#[macro_export]
macro_rules! any_to_ref {
    ( $x:expr, $t:ty ) => {
        $x.unwrap().downcast_ref::<$t>().unwrap()
    };
}
