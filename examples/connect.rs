extern crate td_revent;
extern crate psocket;

use psocket::{ToSocketAddrs};

use td_revent::*;
use self::psocket::TcpSocket;

fn client_read_callback(
    _ev: &mut EventLoop,
    buffer: &mut EventBuffer,
    _data: Option<&mut CellAny>,
) -> RetValue {
    println!("client_read_callback");
    let len = buffer.read.len();
    let data = buffer.read.drain_collect(len);
    println!("data = {:?}", String::from_utf8_lossy(&data));
    RetValue::OK
}

fn client_write_callback(
    _ev: &mut EventLoop,
    _buffer: &mut EventBuffer,
    _data: Option<&mut CellAny>,
) -> RetValue {
    println!("write callback");
    RetValue::OK
}

fn client_end_callback(ev: &mut EventLoop, _buffer: &mut EventBuffer, _data: Option<CellAny>) {
    println!("end callback!!!!!!!!!!!");
    ev.shutdown();
}

//timer return no success(0) will no be repeat
fn time_callback(
    ev: &mut EventLoop,
    _timer: u32,
    data: Option<&mut CellAny>,
) -> (RetValue, u64) {
    println!("time call back");
    let cell_any = data.unwrap();
    {
        let obj = any_to_mut!(cell_any, EventBuffer);
        match obj.socket.check_ready() {
            Err(_) => return (RetValue::OVER, 0),
            Ok(false) => return (RetValue::CONTINUE, 10),
            _ => ()
        }
    };

    println!("Socket is ready");
    let obj = any_unwrap!(cell_any, EventBuffer);
    
    // let obj = any.downcast_mut::<EventBuffer>().unwrap();
    let socket = obj.as_raw_socket();
    let _ = ev.register_socket(
        obj,
        EventEntry::new_event(
            socket,
            FLAG_READ | FLAG_PERSIST,
            Some(client_read_callback),
            Some(client_write_callback),
            Some(client_end_callback),
            None,
        ),
    );

    let _ = ev.send_socket(&socket, b"GET /s?wd=1 HTTP/1.1\r\n\r\n");
    (RetValue::OK, 0)
}

fn main() {
    println!("Starting TEST_ECHO_SERVER");


    let mut event_loop = EventLoop::new().unwrap();

    let mut addrs_iter = "www.baidu.com:80".to_socket_addrs().unwrap();
    let addr = addrs_iter.next().unwrap();
    println!("addr = {:?}", addr);

    let client = TcpSocket::connect_asyn(&addr).unwrap();
    let _ = client.set_nonblocking(true);

    let buffer = event_loop.new_buff(client);
    event_loop.add_timer(EventEntry::new_timer(
        100,
        false,
        Some(time_callback),
        Some(Box::new(buffer)),
    ));

    event_loop.run().unwrap();
    // assert!(unsafe { S_COUNT } == 6);
    println!("SUCCESS END TEST");
}
