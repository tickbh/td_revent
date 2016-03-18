extern crate td_revent;
extern crate net2;
use td_revent::*;
use std::io::prelude::*;
use std::mem;
use std::net::{TcpStream, TcpListener};

struct SocketManger {
    pub listener : TcpListener,
    pub client : TcpStream,
}

static mut s_count : i32 = 0; 

fn client_read_callback(ev : &mut EventLoop, _fd : u32, _ : EventFlags, data : *mut()) -> i32 {
    let sock_mgr : &mut SocketManger = unsafe { &mut *(data as *mut SocketManger) };
    println!("{:?}", sock_mgr.client);
    let mut data : [u8; 1024] = [0; 1024];
    let size = match sock_mgr.client.read(&mut data[..]) {
        Ok(len) => len,
        Err(err) => {
            panic!(format!("{:?}", err))
        },
    };

    println!("size = {:?}", size);
    if size <= 0 {
        ev.del_event(sock_mgr.client.as_fd() as u32, FLAG_READ | FLAG_WRITE);
        // drop(sock_mgr.client);
        return 0;
    }
    let count = unsafe {
        s_count = s_count + 1;
        s_count
    };

    if count >= 6 {
        println!("client close the socket");
        ev.del_event(sock_mgr.client.as_fd() as u32, FLAG_READ | FLAG_WRITE);
        drop(TcpStream::from_fd(sock_mgr.client.as_fd() as i32));
        return 0;
    } else {
        let str = String::from_utf8_lossy(&data[0..size]);
        println!("{:?}", str);
        sock_mgr.client.write(&data[0..size]).unwrap();
    }
    0
}

fn server_read_callback(ev : &mut EventLoop, fd : u32, _ : EventFlags, _data : *mut()) -> i32 {
    println!("server_read_callback");
    let mut socket = TcpStream::from_fd(fd as i32);

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
        drop(socket);
        ev.shutdown();
        return 0;
    }
    let str = String::from_utf8_lossy(&data[0..size]);
    println!("{:?}", str);
    socket.write(&data[0..size]).unwrap();

    mem::forget(socket);
    0
}

fn accept_callback(ev : &mut EventLoop, _fd : u32, _ : EventFlags, data : *mut ()) -> i32 {
    let sock_mgr : &mut SocketManger = unsafe { &mut *(data as *mut SocketManger) };

    let (new_socket, new_attr) = sock_mgr.listener.accept().unwrap();
    let _ = net2::TcpStreamExt::set_nonblocking(&new_socket, false);

    println!("{:?} attr is {:?}", new_socket, new_attr);
    ev.add_event(EventEntry::new(new_socket.as_fd() as u32, FLAG_READ, Some(server_read_callback), Some(data)));
    mem::forget(new_socket);
    0
}

#[test]
pub fn test_base_echo() {
    println!("Starting TEST_ECHO_SERVER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = "127.0.0.1:10009";
    let listener = TcpListener::bind(&addr).unwrap();
    let _ = net2::TcpListenerExt::set_nonblocking(&listener, false);

    let client = TcpStream::connect(&addr).unwrap();
    let _ = net2::TcpStreamExt::set_nonblocking(&client, false);

    let mut sock_mgr = SocketManger { listener : listener, client : client };
    event_loop.add_event(EventEntry::new(sock_mgr.listener.as_fd() as u32, FLAG_READ, Some(accept_callback), Some(&sock_mgr as *const _ as *mut ())));
    event_loop.add_event(EventEntry::new(sock_mgr.client.as_fd() as u32, FLAG_READ, Some(client_read_callback), Some(&sock_mgr as *const _ as *mut ())));

    sock_mgr.client.write(b"hello world").unwrap();

    // mem::forget(listener);
    // mem::forget(client);
    event_loop.run().unwrap();

    assert!(unsafe { s_count } == 6);
}

