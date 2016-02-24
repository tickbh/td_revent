extern crate event;

pub use test::localhost;
mod test_timer;
mod test_echo_server;

mod test {
    use std::net::SocketAddr;
    use std::str::FromStr;

    pub fn localhost() -> SocketAddr {
        let s = format!("127.0.0.1:{}", 1009);
        FromStr::from_str(&s).unwrap()
    }
}
