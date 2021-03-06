

extern crate kernel32;
extern crate winapi;
extern crate ws2_32;
#[cfg(test)]
extern crate rand;

// pub mod selector;

// pub use self::selector::Selector;

pub mod selector;
pub use self::selector::Selector;
pub use super::FromRawArc;

use std::cmp;
use std::io;
use std::time::Duration;

use winapi::*;

mod handle;
mod overlapped;

pub mod iocp;
pub mod net;

pub use self::overlapped::Overlapped;
pub use self::net::{TcpSocketExt, AcceptAddrsBuf};

fn cvt(i: BOOL) -> io::Result<BOOL> {
    if i == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(i)
    }
}

fn dur2ms(dur: Option<Duration>) -> u32 {
    let dur = match dur {
        Some(dur) => dur,
        None => return INFINITE,
    };
    let ms = dur.as_secs().checked_mul(1_000);
    let ms_extra = dur.subsec_nanos() / 1_000_000;
    ms.and_then(|ms| ms.checked_add(ms_extra as u64))
        .map(|ms| cmp::min(u32::max_value() as u64, ms) as u32)
        .unwrap_or(INFINITE - 1)
}
