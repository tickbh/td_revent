
mod socket;

pub use self::socket::{
    Socket,
};

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use self::win::{
    Selector,
};


#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use self::unix::{
    Selector,
};

