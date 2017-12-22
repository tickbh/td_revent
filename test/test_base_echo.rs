extern crate td_revent;
extern crate psocket;


use td_revent::*;
use std::io::prelude::*;
use std::any::Any;
use self::psocket::TcpSocket;

static mut S_COUNT : i32 = 0; 

fn client_read_callback(_ev : &mut EventLoop, _fd : i32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let client = any_to_mut!(data, TcpSocket);
    // let client = data.unwrap().downcast_mut::<TcpSocket>().unwrap();
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
        S_COUNT = S_COUNT + 1;
        S_COUNT
    };

    if count >= 6 {
        println!("client close the socket");
        return RetValue::OVER;
    } else {
        let str = String::from_utf8_lossy(&data[0..size]);
        println!("{:?}", str);
        client.write(&data[0..size]).unwrap();
    }
    RetValue::OK
}

fn server_read_callback(ev : &mut EventLoop, _fd : i32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let socket = any_to_mut!(data, TcpSocket);

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
        return RetValue::OK;
    }
    let str = String::from_utf8_lossy(&data[0..size]);
    println!("{:?}", str);
    socket.write(&data[0..size]).unwrap();
    RetValue::OK
}

fn accept_callback(ev : &mut EventLoop, _fd : i32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    let listener = any_to_mut!(data, TcpSocket);

    let (new_socket, new_attr) = listener.accept().unwrap();
    let _ = new_socket.set_nonblocking(true);

    println!("{:?} attr is {:?}", new_socket, new_attr);
    ev.add_new_event(new_socket.as_raw_socket(), FLAG_READ | FLAG_PERSIST, Some(server_read_callback), Some(Box::new(new_socket)));
    RetValue::OK
}

#[test]
pub fn test_base_echo() {
    println!("Starting TEST_ECHO_SERVER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = "127.0.0.1:10009";
    let listener = TcpSocket::bind(&addr).unwrap();
    let _ = listener.set_nonblocking(true);

    let mut client = TcpSocket::connect(&addr).unwrap();
    let _ = client.set_nonblocking(true);

    client.write(b"hello world").unwrap();

    // let mut sock_mgr = SocketManger { listener : listener, client : client };
    event_loop.add_new_event(listener.as_raw_socket(), FLAG_READ | FLAG_PERSIST, Some(accept_callback), Some(Box::new(listener)));
    event_loop.add_new_event(client.as_raw_socket(), FLAG_READ | FLAG_PERSIST, Some(client_read_callback), Some(Box::new(client)));

    // mem::forget(listener);
    // mem::forget(client);
    event_loop.run().unwrap();

    assert!(unsafe { S_COUNT } == 6);
}

