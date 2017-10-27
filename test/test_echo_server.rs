extern crate td_revent;
extern crate net2;
extern crate psocket;

use td_revent::*;
use std::io::prelude::*;
// use td_revent::{AsFd};
use std::collections::HashMap;
use std::any::Any;
use self::psocket::TcpSocket;
// use std::net::{TcpSocket, TcpSocket};

extern crate libc;

struct SocketManger {
    pub listener : HashMap<u32, TcpSocket>,
    pub clients : HashMap<u32, TcpSocket>,
}

static mut s_count : i32 = 0; 

fn client_read_callback(ev : &mut EventLoop, fd : u32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let socket = data.unwrap().downcast_mut::<TcpSocket>().unwrap();
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
        s_count = s_count + 1;
        s_count
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

fn server_read_callback(ev : &mut EventLoop, fd : u32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    println!("server_read_callback");
    let socket = data.unwrap().downcast_mut::<TcpSocket>().unwrap();

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

fn accept_callback(ev : &mut EventLoop, fd : u32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let listener = data.unwrap().downcast_mut::<TcpSocket>().unwrap();

    let (mut new_socket, new_attr) = listener.accept().unwrap();
    new_socket.set_nonblocking(true);

    println!("{:?} attr is {:?}", new_socket, new_attr);
    ev.add_event(EventEntry::new(new_socket.get_socket_fd() as u32, FLAG_READ | FLAG_PERSIST, Some(server_read_callback), Some(Box::new(new_socket))));
    RetValue::OK
}

#[test]
pub fn test_echo_server() {
    println!("Starting TEST_ECHO_SERVER");
    let mut event_loop = EventLoop::new().unwrap();
    let mut sock_mgr = SocketManger { listener : HashMap::new(), clients : HashMap::new() };

    let addr = "127.0.0.1:10090";
    let mut listener = TcpSocket::bind(&addr).unwrap();
    listener.set_nonblocking(true);
    event_loop.add_event(EventEntry::new(listener.get_socket_fd() as u32, FLAG_READ | FLAG_PERSIST, Some(accept_callback), Some(Box::new(listener))));

    let mut stream = TcpSocket::connect(&addr).unwrap();
    stream.set_nonblocking(true);

    stream.write(b"hello world").unwrap();
    event_loop.add_event(EventEntry::new(stream.get_socket_fd() as u32, FLAG_READ | FLAG_PERSIST, Some(client_read_callback), Some(Box::new(stream))));

    // mem::forget(listener);
    // mem::forget(stream);
    event_loop.run().unwrap();

    assert!(unsafe { s_count } == 6);
}

