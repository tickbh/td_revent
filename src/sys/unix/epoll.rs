#![allow(dead_code)]
use std::io::{Error, Result};
use libc::c_int;
use std::os::unix::io::RawFd;


mod ffi {
    use libc::{c_int};
    use super::EpollEvent;

    extern {
        pub fn epoll_create(size: c_int) -> c_int;
        pub fn epoll_ctl(epfd: c_int, op: c_int, fd: c_int, event: *const EpollEvent) -> c_int;
        pub fn epoll_wait(epfd: c_int, events: *mut EpollEvent, max_events: c_int, timeout: c_int) -> c_int;
    }
}

bitflags!(
    #[repr(C)]
    flags EpollEventKind: u32 {
        const EPOLLIN = 0x001,
        const EPOLLPRI = 0x002,
        const EPOLLOUT = 0x004,
        const EPOLLRDNORM = 0x040,
        const EPOLLRDBAND = 0x080,
        const EPOLLWRNORM = 0x100,
        const EPOLLWRBAND = 0x200,
        const EPOLLMSG = 0x400,
        const EPOLLERR = 0x008,
        const EPOLLHUP = 0x010,
        const EPOLLRDHUP = 0x2000,
        const EPOLLWAKEUP = 1 << 29,
        const EPOLLONESHOT = 1 << 30,
        const EPOLLET = 1 << 31,
    }
);

// impl fmt::Debug for EpollEventKind {
//     fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
//         let variants = [
//             (EPOLLIN,       "EPOLLIN"),
//             (EPOLLPRI,      "EPOLLPRI"),
//             (EPOLLOUT,      "EPOLLOUT"),
//             (EPOLLRDNORM,   "EPOLLRDNORM"),
//             (EPOLLRDBAND,   "EPOLLRDBAND"),
//             (EPOLLWRNORM,   "EPOLLWRNORM"),
//             (EPOLLWRBAND,   "EPOLLWRBAND"),
//             (EPOLLMSG,      "EPOLLMSG"),
//             (EPOLLERR,      "EPOLLERR"),
//             (EPOLLHUP,      "EPOLLHUP"),
//             (EPOLLRDHUP,    "EPOLLRDHUP"),
//             (EPOLLWAKEUP,   "EPOLLWAKEUP"),
//             (EPOLLONESHOT,  "EPOLLONESHOT"),
//             (EPOLLET,       "EPOLLET")];

//         let mut first = true;

//         for &(val, name) in variants.iter() {
//             if self.contains(val) {
//                 if first {
//                     first = false;
//                     try!(write!(fmt, "{}", name));
//                 } else {
//                     try!(write!(fmt, "|{}", name));
//                 }
//             }
//         }

//         Ok(())
//     }
// }


#[derive(Clone, Copy)]
#[repr(C)]
pub enum EpollOp {
    EpollCtlAdd = 1,
    EpollCtlDel = 2,
    EpollCtlMod = 3
}

#[cfg(all(target_os = "android", not(target_arch = "x86_64")))]
#[derive(Clone, Copy)]
#[repr(C)]
pub struct EpollEvent {
    pub events: EpollEventKind,
    pub data: u64
}

#[cfg(all(target_os = "android", not(target_arch = "x86_64")))]
#[test]
fn test_epoll_event_size() {
    use std::mem::size_of;
    assert_eq!(size_of::<EpollEvent>(), 16);
}

#[cfg(any(not(target_os = "android"), target_arch = "x86_64"))]
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct EpollEvent {
    pub events: EpollEventKind,
    pub data: u64
}

#[inline]
pub fn epoll_create() -> Result<RawFd> {
    let res = unsafe { ffi::epoll_create(1024) };

    if res < 0 {
        return Err(Error::last_os_error());
    }

    Ok(res)
}

#[inline]
pub fn epoll_ctl(epfd: RawFd, op: EpollOp, fd: RawFd, event: &EpollEvent) -> Result<()> {
    let res = unsafe { ffi::epoll_ctl(epfd, op as c_int, fd, event as *const EpollEvent) };
    if res != 0 {
        return Err(Error::last_os_error());
    }
    Ok(())
}

#[inline]
pub fn epoll_wait(epfd: RawFd, events: &mut [EpollEvent], timeout_ms: isize) -> Result<usize> {
    let res = unsafe {
        ffi::epoll_wait(epfd, events.as_mut_ptr(), events.len() as c_int, timeout_ms as c_int)
    };

    if res < 0 {
        return Err(Error::last_os_error());
    }

    Ok(res as usize)
}
