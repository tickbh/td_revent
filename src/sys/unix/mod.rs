
#[cfg(any(target_os = "linux", target_os = "android"))]
mod epoll;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub use self::epoll::{Events, Selector};

#[cfg(any(target_os = "bitrig", target_os = "dragonfly", target_os = "freebsd",
            target_os = "ios", target_os = "macos", target_os = "netbsd", target_os = "openbsd"))]
mod kqueue;
#[cfg(any(target_os = "bitrig", target_os = "dragonfly", target_os = "freebsd",
            target_os = "ios", target_os = "macos", target_os = "netbsd", target_os = "openbsd"))]
pub use self::kqueue::{Events, Selector};

pub fn from_nix_error(err: ::nix::Error) -> ::std::io::Error {
    ::std::io::Error::from_raw_os_error(err.errno() as i32)
}
