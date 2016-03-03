#[cfg(unix)] use std::os::unix::prelude::*;
#[cfg(windows)] use std::os::windows::prelude::*;
#[cfg(windows)] use winapi::*;

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use self::win::{
    Selector,
};


#[cfg(not(windows))]
mod unix;

#[cfg(not(windows))]
pub use self::unix::{
    Selector,
};

#[doc(hidden)]
pub trait AsFd {
    fn as_fd(&self) -> i32;
}

#[cfg(unix)]
impl<T: AsRawFd> AsFd for T {
    fn as_fd(&self) -> i32 { self.as_raw_fd() as i32 }
}
#[cfg(windows)]
impl<T: AsRawSocket> AsFd for T {
    fn as_fd(&self) -> i32 { self.as_raw_socket() as i32 }
}

pub trait FromFd<T> {
    fn from_fd(fd : i32) -> T;
}

#[cfg(unix)]
impl<T: FromRawFd> FromFd<T> for T {
    fn from_fd(fd : i32) -> T { unsafe { T::from_raw_fd(fd as RawFd) } }
}
#[cfg(windows)]
impl<T: FromRawSocket> FromFd<T> for T {
    fn from_fd(fd : i32) -> T { unsafe { T::from_raw_socket(fd as SOCKET) } }
}