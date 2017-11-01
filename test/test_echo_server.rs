extern crate td_revent;
extern crate net2;
extern crate psocket;

use td_revent::*;
use std::io::prelude::*;
use std::any::Any;
use self::psocket::TcpSocket;

extern crate libc;

static mut S_COUNT : i32 = 0; 

fn client_read_callback(_ev : &mut EventLoop, _fd : i32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let socket = any_to_mut!(data, TcpSocket);
    println!("{:?}", socket);
    let mut data : [u8; 1024] = [0; 1024];
    let size = match socket.read(&mut data[..]) {
        Ok(len) => len,
        Err(err) => {
            panic!(format!("{:?}", err))
        },
    };
    if size <= 0 {
        return RetValue::OVER;
    }
    let count = unsafe {
        S_COUNT = S_COUNT + 1;
        S_COUNT
    };

    if count >= 6 {
        // panic!("close socket received count is {}", count);
        println!("client close the socket");
        return RetValue::OVER;
    } else {
        let str = String::from_utf8_lossy(&data[0..size]);
        println!("{:?}", str);
        socket.write(&data[0..size]).unwrap();
    }
    RetValue::OK
}

fn server_read_callback(ev : &mut EventLoop, _fd : i32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    println!("server_read_callback");
    let socket = any_to_mut!(data, TcpSocket);

    println!("{:?}", socket);
    let mut data : [u8; 1024] = [0; 1024];
    let size = match socket.read(&mut data[..]) {
        Ok(len) => len,
        Err(err) => {
            panic!(format!("{:?}", err))
        },
    };

    if size <= 0 {
        drop(socket);
        ev.shutdown();
        return RetValue::OK;
    }
    let str = String::from_utf8_lossy(&data[0..size]);
    println!("{:?}", str);
    socket.write(&data[0..size]).unwrap();

    RetValue::OK
}

fn accept_callback(ev : &mut EventLoop, _fd : i32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let listener = any_to_mut!(data, TcpSocket);

    let (mut new_socket, new_attr) = listener.accept().unwrap();
    let _ = new_socket.set_nonblocking(true);

    println!("{:?} attr is {:?}", new_socket, new_attr);
    ev.add_new_event(new_socket.get_socket_fd(), FLAG_READ | FLAG_PERSIST, Some(server_read_callback), Some(Box::new(new_socket)));
    RetValue::OK
}

#[test]
pub fn test_echo_server() {
    println!("Starting TEST_ECHO_SERVER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = "127.0.0.1:10090";
    let mut listener = TcpSocket::bind(&addr).unwrap();
    let _ = listener.set_nonblocking(true);
    event_loop.add_new_event(listener.get_socket_fd(), FLAG_READ | FLAG_PERSIST, Some(accept_callback), Some(Box::new(listener)));

    let mut stream = TcpSocket::connect(&addr).unwrap();
    let _ = stream.set_nonblocking(true);

    stream.write(b"hello world").unwrap();
    event_loop.add_new_event(stream.get_socket_fd(), FLAG_READ | FLAG_PERSIST, Some(client_read_callback), Some(Box::new(stream)));

    // mem::forget(listener);
    // mem::forget(stream);
    event_loop.run().unwrap();

    assert!(unsafe { S_COUNT } == 6);
}

