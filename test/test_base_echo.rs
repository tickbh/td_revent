extern crate td_revent;
extern crate net2;
extern crate psocket;

use td_revent::*;
use std::io::prelude::*;
use std::mem;
use std::any::Any;
use self::psocket::TcpSocket;

static mut s_count : i32 = 0; 

fn client_read_callback(ev : &mut EventLoop, _fd : u32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let client = data.unwrap().downcast_mut::<TcpSocket>().unwrap();
    println!("{:?}", client);
    let mut data : [u8; 1024] = [0; 1024];
    let size = match client.read(&mut data[..]) {
        Ok(len) => len,
        Err(err) => {
            panic!(format!("{:?}", err))
        },
    };

    println!("size = {:?}", size);
    if size <= 0 {
        return RetValue::OVER;
    }
    let count = unsafe {
        s_count = s_count + 1;
        s_count
    };

    if count >= 6 {
        println!("client close the socket");
        return RetValue::OVER;
    } else {
        let str = String::from_utf8_lossy(&data[0..size]);
        println!("{:?}", str);
        client.write(&data[0..size]).unwrap();
    }
    RetValue::NONE
}

fn server_read_callback(ev : &mut EventLoop, fd : u32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let socket = data.unwrap().downcast_mut::<TcpSocket>().unwrap();

    println!("{:?}", socket);

    let mut data : [u8; 1024] = [0; 1024];
    let size = match socket.read(&mut data[..]) {
        Ok(len) => len,
        Err(err) => {
            panic!(format!("{:?}", err))
        },
    };
    println!("size = {:?}", size);
    

    if size <= 0 {
        ev.shutdown();
        return RetValue::NONE;
    }
    let str = String::from_utf8_lossy(&data[0..size]);
    println!("{:?}", str);
    socket.write(&data[0..size]).unwrap();
    RetValue::NONE
}

fn accept_callback(ev : &mut EventLoop, _fd : u32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let listener = data.unwrap().downcast_mut::<TcpSocket>().unwrap();

    let (mut new_socket, new_attr) = listener.accept().unwrap();
    new_socket.set_nonblocking(true);

    println!("{:?} attr is {:?}", new_socket, new_attr);
    ev.add_event(EventEntry::new(new_socket.get_socket_fd() as u32, FLAG_READ | FLAG_PERSIST, Some(server_read_callback), Some(Box::new(new_socket))));
    RetValue::NONE
}

#[test]
pub fn test_base_echo() {
    println!("Starting TEST_ECHO_SERVER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = "127.0.0.1:10009";
    let mut listener = TcpSocket::bind(&addr).unwrap();
    listener.set_nonblocking(true);

    let mut client = TcpSocket::connect(&addr).unwrap();
    listener.set_nonblocking(true);

    client.write(b"hello world").unwrap();

    // let mut sock_mgr = SocketManger { listener : listener, client : client };
    event_loop.add_event(EventEntry::new(listener.get_socket_fd() as u32, FLAG_READ | FLAG_PERSIST, Some(accept_callback), Some(Box::new(listener))));
    event_loop.add_event(EventEntry::new(client.get_socket_fd() as u32, FLAG_READ | FLAG_PERSIST, Some(client_read_callback), Some(Box::new(client))));

    // mem::forget(listener);
    // mem::forget(client);
    event_loop.run().unwrap();

    assert!(unsafe { s_count } == 6);
}

