extern crate td_revent;
use td_revent::*;
use td_revent::sys::*;
use std::collections::HashMap;

extern crate libc;

struct SocketManger {
    pub socks : HashMap<u64, Socket>,
}

static mut s_count : i32 = 0; 

fn client_read_callback(ev : *mut EventLoop, fd : u64, _ : EventFlags, data : *mut()) -> i32 {
    let sock_mgr : &mut SocketManger = unsafe { &mut *(data as *mut SocketManger) };
    let socket = sock_mgr.socks[&fd].clone();  
    println!("{:?}", socket);
    let mut data : [u8; 1024] = [0; 1024];
    let size = match socket.recv_into(&mut data[..], 0) {
        Ok(len) => len,
        Err(err) => {
            panic!(format!("{:?}", err))
        },
    };
    if size <= 0 {
        socket.close().unwrap();
        return 0;
    }
    let count = unsafe {
        s_count = s_count + 1;
        s_count
    };

    if count >= 6 {
        // panic!("close socket received count is {}", count);
        println!("client close the socket");
        socket.close().unwrap();
        let el : &mut EventLoop = unsafe { &mut *ev };
        el.del_event(socket.fileno() as u64, FLAG_READ | FLAG_WRITE);
    } else {
        let str = String::from_utf8_lossy(&data[0..size]);
        println!("{:?}", str);
        socket.send(&data[0..size], 0).unwrap();
    }
    0
}

fn server_read_callback(ev : *mut EventLoop, fd : u64, _ : EventFlags, data : *mut()) -> i32 {
    println!("server_read_callback");
    let sock_mgr : &mut SocketManger = unsafe { &mut *(data as *mut SocketManger) };
    let socket = sock_mgr.socks[&fd].clone();  

    println!("{:?}", socket);
    let mut data : [u8; 1024] = [0; 1024];
    let size = match socket.recv_into(&mut data[..], 0) {
        Ok(len) => len,
        Err(err) => {
            panic!(format!("{:?}", err))
        },
    };

    if size <= 0 {
        socket.close().unwrap();
        let el : &mut EventLoop = unsafe { &mut *ev };
        el.shutdown();
        return 0;
    }
    let str = String::from_utf8_lossy(&data[0..size]);
    println!("{:?}", str);
    socket.send(&data[0..size], 0).unwrap();
    0
}

fn accept_callback(ev : *mut EventLoop, fd : u64, _ : EventFlags, data : *mut ()) -> i32 {
    let sock_mgr : &mut SocketManger = unsafe { &mut *(data as *mut SocketManger) };
    let listener = sock_mgr.socks[&fd].clone();    
    let (new_socket, new_attr) = listener.accept().unwrap();
    new_socket.set_non_blocking().unwrap();
    println!("{:?} attr is {:?}", new_socket, new_attr);
    let el : &mut EventLoop = unsafe { &mut *ev };
    el.add_event(EventEntry::new(new_socket.fileno() as u64, FLAG_READ, Some(server_read_callback), Some(data)));
    sock_mgr.socks.insert(new_socket.fileno() as u64, new_socket);
    0
}

#[test]
pub fn test_echo_server() {
    println!("Starting TEST_ECHO_SERVER");
    let mut event_loop = EventLoop::new().unwrap();
    let mut sock_mgr = SocketManger { socks : HashMap::new() };

    let addr = "127.0.0.1:1009";
    let listener = Socket::new(libc::AF_INET, libc::SOCK_STREAM, 0).unwrap();
    // listener.bind(format!("{}:{}", addr.ip(), addr.port()).as_ref()).unwrap();
    listener.bind(&addr).unwrap();
    listener.listen(10).unwrap();
    listener.set_non_blocking().unwrap();
    event_loop.add_event(EventEntry::new(listener.fileno() as u64, FLAG_READ, Some(accept_callback), Some(&sock_mgr as *const _ as *mut ())));

    let client = Socket::new(libc::AF_INET, libc::SOCK_STREAM, 0).unwrap();
    client.connect(&addr).unwrap();
    client.send(b"hello world", 0).unwrap();
    client.set_non_blocking().unwrap();
    event_loop.add_event(EventEntry::new(client.fileno() as u64, FLAG_READ, Some(client_read_callback), Some(&sock_mgr as *const _ as *mut ())));

    sock_mgr.socks.insert(listener.fileno() as u64, listener);
    sock_mgr.socks.insert(client.fileno() as u64, client);

    event_loop.run().unwrap();

    assert!(unsafe { s_count } == 6);
}

