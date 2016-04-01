extern crate td_revent;
extern crate net2;
use td_revent::*;
use std::io::prelude::*;
// use td_revent::{AsFd};
use std::collections::HashMap;
use std::net::{TcpStream, TcpListener};

extern crate libc;

struct SocketManger {
    pub listener : HashMap<u32, TcpListener>,
    pub clients : HashMap<u32, TcpStream>,
}

static mut s_count : i32 = 0; 

fn client_read_callback(ev : &mut EventLoop, fd : u32, _ : EventFlags, data : *mut()) -> i32 {
    let sock_mgr : &mut SocketManger = unsafe { &mut *(data as *mut SocketManger) };
    let mut socket = sock_mgr.clients.remove(&fd).unwrap();
    println!("{:?}", socket);
    let mut data : [u8; 1024] = [0; 1024];
    let size = match socket.read(&mut data[..]) {
        Ok(len) => len,
        Err(err) => {
            panic!(format!("{:?}", err))
        },
    };
    if size <= 0 {
        ev.del_event(socket.as_fd() as u32, FLAG_READ | FLAG_WRITE);
        drop(socket);
        return 0;
    }
    let count = unsafe {
        s_count = s_count + 1;
        s_count
    };

    if count >= 6 {
        // panic!("close socket received count is {}", count);
        println!("client close the socket");
        ev.del_event(socket.as_fd() as u32, FLAG_READ | FLAG_WRITE);
        drop(socket);
        return 0;
    } else {
        let str = String::from_utf8_lossy(&data[0..size]);
        println!("{:?}", str);
        socket.write(&data[0..size]).unwrap();
    }
    sock_mgr.clients.insert(fd, socket);
    0
}

fn server_read_callback(ev : &mut EventLoop, fd : u32, _ : EventFlags, data : *mut()) -> i32 {
    println!("server_read_callback");
    let sock_mgr : &mut SocketManger = unsafe { &mut *(data as *mut SocketManger) };
    let mut socket = sock_mgr.clients.remove(&fd).unwrap();

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
        return 0;
    }
    let str = String::from_utf8_lossy(&data[0..size]);
    println!("{:?}", str);
    socket.write(&data[0..size]).unwrap();
    sock_mgr.clients.insert(fd, socket);

    0
}

fn accept_callback(ev : &mut EventLoop, fd : u32, _ : EventFlags, data : *mut ()) -> i32 {
    let sock_mgr : &mut SocketManger = unsafe { &mut *(data as *mut SocketManger) };
    let listener = sock_mgr.listener.remove(&fd).unwrap();

    let (new_socket, new_attr) = listener.accept().unwrap();
    let _ = net2::TcpStreamExt::set_nonblocking(&new_socket, false);
    sock_mgr.listener.insert(fd, listener);

    println!("{:?} attr is {:?}", new_socket, new_attr);
    ev.add_event(EventEntry::new(new_socket.as_fd() as u32, FLAG_READ, Some(server_read_callback), Some(data)));
    sock_mgr.clients.insert(new_socket.as_fd() as u32, new_socket);
    0
}

#[test]
pub fn test_echo_server() {
    println!("Starting TEST_ECHO_SERVER");
    let mut event_loop = EventLoop::new().unwrap();
    let mut sock_mgr = SocketManger { listener : HashMap::new(), clients : HashMap::new() };

    let addr = "127.0.0.1:10090";
    let listener = TcpListener::bind(&addr).unwrap();
    let _ = net2::TcpListenerExt::set_nonblocking(&listener, false);
    event_loop.add_event(EventEntry::new(listener.as_fd() as u32, FLAG_READ, Some(accept_callback), Some(&sock_mgr as *const _ as *mut ())));

    let mut stream = TcpStream::connect(&addr).unwrap();
    let _ = net2::TcpStreamExt::set_nonblocking(&stream, false);

    stream.write(b"hello world").unwrap();
    event_loop.add_event(EventEntry::new(stream.as_fd() as u32, FLAG_READ, Some(client_read_callback), Some(&sock_mgr as *const _ as *mut ())));

    sock_mgr.listener.insert(listener.as_fd() as u32, listener);
    sock_mgr.clients.insert(stream.as_fd() as u32, stream);
    // mem::forget(listener);
    // mem::forget(stream);
    event_loop.run().unwrap();

    assert!(unsafe { s_count } == 6);
}

