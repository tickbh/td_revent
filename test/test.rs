extern crate td_revent;

mod test_timer;
mod test_echo_server;
mod test_base_echo;

pub struct TestA {
    pub data: Option<Box<Any>>,
}

fn main() {
    let a = TestA {
        data: Some(Box::new("12345")),
    };
    println!("ok");
}
