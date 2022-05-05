extern crate td_revent;
extern crate psocket;


use td_revent::*;
use std::io::prelude::*;
use std::io::Result;
use self::psocket::TcpSocket;

static mut S_COUNT: i32 = 0;

fn client_read_callback(
    ev: &mut EventLoop,
    buffer: &mut EventBuffer,
    _data: Option<&mut CellAny>,
) -> RetValue {
    let len = buffer.read.len();
    assert!(len > 0);
    let data = buffer.read.drain_collect(len);

    let count = unsafe {
        S_COUNT = S_COUNT + 1;
        S_COUNT
    };

    if count >= 6 {
        return RetValue::OVER;
    } else {
        let _ = ev.send_socket(&buffer.as_raw_socket(), &data[..]);
    }
    RetValue::OK
}

fn server_read_callback(
    ev: &mut EventLoop,
    buffer: &mut EventBuffer,
    _data: Option<&mut CellAny>,
) -> RetValue {
    let len = buffer.read.len();
    let data = buffer.read.drain_collect(len);
    let _ = ev.send_socket(&buffer.as_raw_socket(), &data[..]);
    RetValue::OK
}



fn server_end_callback(ev: &mut EventLoop, _buffer: &mut EventBuffer, _data: Option<CellAny>) {
    ev.shutdown();
}

fn client_end_callback(_ev: &mut EventLoop, _buffer: &mut EventBuffer, _data: Option<CellAny>) {
}


fn accept_callback(
    ev: &mut EventLoop,
    tcp: Result<TcpSocket>,
    _data: Option<&mut CellAny>,
) -> RetValue {
    let new_socket = tcp.unwrap();

    let socket = new_socket.as_raw_socket();
    let buffer = ev.new_buff(new_socket);
    let _ = ev.register_socket(
        buffer,
        EventEntry::new_event(
            socket,
            EventFlags::FLAG_READ | EventFlags::FLAG_PERSIST,
            Some(server_read_callback),
            None,
            Some(server_end_callback),
            None,
        ),
    );
    RetValue::OK
}

fn main() {
    println!("Starting TEST_ECHO_SERVER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = "127.0.0.1:10009";
    let listener = TcpSocket::bind(&addr).unwrap();
    let _ = listener.set_nonblocking(true);

    let mut client = TcpSocket::connect(&addr).unwrap();
    let _ = client.set_nonblocking(true);

    client.write(b"hello world. ").unwrap();

    let socket = listener.as_raw_socket();
    let buffer = event_loop.new_buff(listener);
    let _ = event_loop.register_socket(
        buffer,
        EventEntry::new_accept(
            socket,
            EventFlags::FLAG_READ | EventFlags::FLAG_PERSIST | EventFlags::FLAG_ACCEPT,
            Some(accept_callback),
            None,
            None,
        ),
    );

    let socket = client.as_raw_socket();
    let buffer = event_loop.new_buff(client);
    let _ = event_loop.register_socket(
        buffer,
        EventEntry::new_event(
            socket,
            EventFlags::FLAG_READ | EventFlags::FLAG_PERSIST,
            Some(client_read_callback),
            None,
            Some(client_end_callback),
            None,
        ),
    );

    event_loop.run().unwrap();
    assert!(unsafe { S_COUNT } == 6);
    println!("SUCCESS END TEST");
}
