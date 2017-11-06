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
mod event_buffer;

pub use timer::Timer;
pub use event_loop::{EventLoop, EventLoopConfig, RetValue};

pub use event_buffer::{Buffer, EventBuffer};

pub mod sys;

pub use event_flags::{EventFlags, FLAG_TIMEOUT, FLAG_READ, FLAG_WRITE, FLAG_PERSIST, FLAG_ERROR, FLAG_ACCEPT};
pub use event_entry::{EventEntry, ACCEPT_CB, EVENT_CB, TIMER_CB};
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
