#![allow(trivial_casts)]
#![allow(unused_unsafe)]
#![allow(dead_code)]
#![allow(unused_imports)]

use libc;
extern crate num;

#[cfg(windows)]
use super::win::winsock;

pub use libc::{
    AF_INET, AF_INET6, SOCK_STREAM, SOCK_DGRAM, SOCK_RAW,
    IPPROTO_IP, IPPROTO_IPV6, IPPROTO_TCP, TCP_NODELAY,
    SOL_SOCKET, SO_KEEPALIVE, SO_ERROR,
    SO_REUSEADDR, SO_BROADCAST, SHUT_WR, IP_MULTICAST_LOOP,
    IP_ADD_MEMBERSHIP, IP_DROP_MEMBERSHIP,
    IPV6_ADD_MEMBERSHIP, IPV6_DROP_MEMBERSHIP,
    IP_MULTICAST_TTL, IP_TTL, IP_HDRINCL, SHUT_RD,
    IPPROTO_RAW,
};


use std::iter::{FromIterator, repeat, };
use std::io::{Error, ErrorKind, Result,};
use std::mem;
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs, SocketAddrV4};
use std::ops::Drop;
use std::vec::{Vec,};
use std::ptr;
use std::clone::Clone;

use libc::{
    c_void, size_t, in_addr, sockaddr, sockaddr_in, socklen_t,
    c_int, c_uchar,c_char, c_ushort,

    socket, setsockopt, bind, send, recv, recvfrom,
    close,
    listen, sendto, accept, connect, getsockname,
    shutdown,
};


#[cfg(windows)]
type SOCKET = libc::SOCKET;
#[cfg(not(windows))]
type SOCKET = c_int;

#[cfg(windows)]
type BuffLen = c_int;
#[cfg(not(windows))]
type BuffLen = size_t;

macro_rules! _try {
    ( $fun:ident, $( $x:expr ),* ) => {{
        let value = unsafe { $fun($($x,)*) };
        if value as i32 == -1 {
            return Err(Error::last_os_error());
        }
        value
    }};
}

#[inline]
pub fn htons(hostshort: u16) -> u16 {
    hostshort.to_be()
}

#[inline]
pub fn ntohs(netshort: u16) -> u16 {
    u16::from_be(netshort)
}

#[inline]
pub fn htonl(hostlong: u32) -> u32 {
    hostlong.to_be()
}

#[inline]
pub fn ntohl(netlong: u32) -> u32 {
    u32::from_be(netlong)
}

#[derive(Debug, Clone)]
pub struct Socket {
    fd: i32,
}

fn tosocketaddrs_to_socketaddr<T: ToSocketAddrs + ?Sized>(address: &T) -> Result<SocketAddr> {
    let addresses: Vec<SocketAddr> = FromIterator::from_iter(try!(address.to_socket_addrs()));

    match addresses.len() {
        1 => {
            Ok(addresses[0])
        },
        // TODO is this really possible?
        n => Err(Error::new(
            ErrorKind::InvalidInput,
            &format!(
                "Incorrect number of IP addresses passed, \
                1 address expected, got {}", n)[..],
        ))
    }
}


fn tosocketaddrs_to_sockaddr<T: ToSocketAddrs + ?Sized>(address: &T) -> Result<sockaddr> {
    Ok(socketaddr_to_sockaddr(&try!(tosocketaddrs_to_socketaddr(address))))
}

impl Socket {
    #[cfg(windows)]
    pub fn new(socket_family: i32, socket_type: i32, _protocol: i32) -> Result<Socket> {
        winsock::init();
        let socket = try!(unsafe {
            match winsock::WSASocketW(socket_family, socket_type, 0, ptr::null_mut(), 0,
                                winsock::WSA_FLAG_OVERLAPPED) {
                libc::INVALID_SOCKET => Err(Error::last_os_error()),
                n => Ok(Socket { fd : n as i32 }),
            }
        });
        Ok(socket)
    }

    #[cfg(not(windows))]
    pub fn new(socket_family: i32, socket_type: i32, protocol: i32) -> Result<Socket> {
        let fd = _try!(socket, socket_family, socket_type, protocol);
        Ok(Socket { fd: fd as i32 })
    }

    pub fn new_by_fd(fd : i32) -> Socket {
        Socket { fd : fd }
    }

    pub fn fileno(&self) -> i32 {
        self.fd
    }

    pub fn setsockopt<T>(&self, level: i32, name: i32, value: T) -> Result<()> {
        unsafe {
            let value = &value as *const T as *const c_void;
            _try!(
                setsockopt,
                self.fd as SOCKET, level, name, value, mem::size_of::<T>() as socklen_t);
        }
        Ok(())
    }

    pub fn bind<T: ToSocketAddrs + ?Sized>(&self, address: &T) -> Result<()> {
        let sa = try!(tosocketaddrs_to_sockaddr(address));
        _try!(bind, self.fd as SOCKET, &sa, num::cast(mem::size_of::<sockaddr>()).unwrap());
        Ok(())
    }

    pub fn getsockname(&self) -> Result<SocketAddr> {
        let mut sa: sockaddr = unsafe { mem::zeroed() };
        let mut len: socklen_t = mem::size_of::<sockaddr>() as socklen_t;
        _try!(getsockname, self.fd as SOCKET,
              &mut sa as *mut sockaddr, &mut len as *mut socklen_t);
        assert_eq!(len, mem::size_of::<sockaddr>() as socklen_t);
        Ok(sockaddr_to_socketaddr(&sa))
    }

    pub fn sendto<T: ToSocketAddrs + ?Sized>(&self, buffer: &[u8], flags: i32, address: &T)
            -> Result<usize> {
        let sa = try!(tosocketaddrs_to_sockaddr(address));
        let sent = _try!(
            sendto, self.fd as SOCKET, buffer.as_ptr() as *const c_void,
            buffer.len() as BuffLen, flags, &sa as *const sockaddr,
            num::cast(mem::size_of::<sockaddr>()).unwrap());
        Ok(sent as usize)
    }

    pub fn send(&self, buffer: &[u8], flags: i32)
            -> Result<usize> {
        let sent = _try!(
            send, self.fd as SOCKET, buffer.as_ptr() as *const c_void, buffer.len() as BuffLen, flags);
        Ok(sent as usize)
    }


    /// Receives data from a remote socket and returns it with the address of the socket.
    pub fn recvfrom(&self, bytes: usize, flags: i32) -> Result<(SocketAddr, Box<[u8]>)> {
        let mut a = Vec::with_capacity(bytes);

        // This is needed to get some actual elements in the vector, not just a capacity
        a.extend(repeat(0u8).take(bytes));

        let (socket_addr, received) = try!(self.recvfrom_into(&mut a[..], flags));

        a.truncate(received);
        Ok((socket_addr, a.into_boxed_slice()))
    }

    pub fn recvfrom_into(&self, buffer: &mut [u8], flags: i32) -> Result<(SocketAddr, usize)> {
        let mut sa: sockaddr = unsafe { mem::zeroed() };
        let mut sa_len: socklen_t = mem::size_of::<sockaddr>() as socklen_t;
        let received = _try!(
            recvfrom, self.fd as SOCKET, buffer.as_ptr() as *mut c_void, buffer.len() as BuffLen, flags,
            &mut sa as *mut sockaddr, &mut sa_len as *mut socklen_t);
        assert_eq!(sa_len, mem::size_of::<sockaddr>() as socklen_t);
        Ok((sockaddr_to_socketaddr(&sa), received as usize))
    }


    /// Returns up to `bytes` bytes received from the remote socket.
    pub fn recv(&self, bytes: usize, flags: i32) -> Result<Box<[u8]>> {
        let mut a = Vec::with_capacity(bytes);

        // This is needed to get some actual elements in the vector, not just a capacity
        a.extend(repeat(0u8).take(bytes));

        let received = try!(self.recv_into(&mut a[..], flags));

        a.truncate(received);
        Ok(a.into_boxed_slice())
    }

    /// Similar to `recv` but receives to predefined buffer and returns the number
    /// of bytes read.
    pub fn recv_into(&self, buffer: &mut [u8], flags: i32) -> Result<usize> {
        let received = _try!(recv, self.fd as SOCKET, buffer.as_ptr() as *mut c_void, buffer.len() as BuffLen, flags);
        Ok(received as usize)
    }

    pub fn connect<T: ToSocketAddrs + ?Sized>(&self, toaddress: &T) -> Result<()> {
        let address = try!(tosocketaddrs_to_sockaddr(toaddress));
        _try!(connect, self.fd as SOCKET, &address as *const sockaddr, num::cast(mem::size_of::<sockaddr>()).unwrap());
        Ok(())
    }

    pub fn listen(&self, backlog: i32) -> Result<()> {
        _try!(listen, self.fd as SOCKET, backlog);
        Ok(())
    }

    pub fn accept(&self) -> Result<(Socket, SocketAddr)> {
        let mut sa: sockaddr = unsafe { mem::zeroed() };
        let mut sa_len: socklen_t = mem::size_of::<sockaddr>() as socklen_t;

        let fd = _try!(
            accept, self.fd as SOCKET, &mut sa as *mut sockaddr, &mut sa_len as *mut socklen_t);
        assert_eq!(sa_len, mem::size_of::<sockaddr>() as socklen_t);
        Ok((Socket { fd: fd as i32 }, sockaddr_to_socketaddr(&sa)))
    }

    #[cfg(not(windows))]
    pub fn close(&self) -> Result<()> {
        _try!(close, self.fd as SOCKET);
        Ok(())
    }

    #[cfg(windows)]
    pub fn close(&self) -> Result<()> {
        use libc::closesocket;
        let _ = _try!(closesocket, self.fd as SOCKET);
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn set_non_blocking(&self) -> Result<()> {
        use libc::fcntl;
        let mut cur_flag = _try!(fcntl, self.fd as SOCKET, libc::F_GETFL);
        cur_flag |= libc::O_NONBLOCK;
        _try!(fcntl, self.fd as SOCKET, libc::F_SETFL, cur_flag);
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn set_blocking(&self) -> Result<()> {
        use libc::fcntl;
        let mut cur_flag = _try!(fcntl, self.fd as SOCKET, libc::F_GETFL);
        cur_flag &= !libc::O_NONBLOCK;
        _try!(fcntl, self.fd as SOCKET, libc::F_SETFL, cur_flag);
        Ok(())
        
    }


    #[cfg(windows)]
    pub fn set_non_blocking(&self) -> Result<()> {
        use libc::ioctlsocket;
        let mut set = 1 as libc::c_ulong;
        let _ = _try!(ioctlsocket, self.fd as SOCKET, libc::FIONBIO, &mut set);
        Ok(())
    }

    #[cfg(windows)]
    pub fn set_blocking(&self) -> Result<()> {
        use libc::ioctlsocket;
        let mut set = 0 as libc::c_ulong;
        let _ = _try!(ioctlsocket, self.fd as SOCKET, libc::FIONBIO, &mut set);
        Ok(())
    }

    pub fn shutdown(&self, how: i32) -> Result<()> {
        _try!(shutdown, self.fd as SOCKET, how as c_int);
        Ok(())
    }
}


impl Drop for Socket {
    fn drop(&mut self) {
        // let _ = self.close();
    }
}

fn socketaddr_to_sockaddr(addr: &SocketAddr) -> sockaddr {
    unsafe {
        match *addr {
            SocketAddr::V4(v4) => {
                let mut sa: sockaddr_in = mem::zeroed();
                sa.sin_family = num::cast(AF_INET).unwrap();
                sa.sin_port = htons(v4.port());
                sa.sin_addr = *(&v4.ip().octets() as *const u8 as *const in_addr);
                *(&sa as *const sockaddr_in as *const sockaddr)
            },
            SocketAddr::V6(_) => {
                panic!("Not supported");
                /*
                let mut sa: sockaddr_in6 = mem::zeroed();
                sa.sin6_family = AF_INET6 as u16;
                sa.sin6_port = htons(v6.port());
                (&sa as *const sockaddr_in6 as *const sockaddr)
                */
            },
        }
    }
}

fn sockaddr_to_socketaddr(sa: &sockaddr) -> SocketAddr {
    match sa.sa_family as i32 {
        AF_INET => {
            let sin: &sockaddr_in = unsafe { mem::transmute(sa) };
            let ip_parts: [u8; 4] = unsafe { mem::transmute(sin.sin_addr) };
            SocketAddr::V4(
                SocketAddrV4::new(Ipv4Addr::new(
                    ip_parts[0],
                    ip_parts[1],
                    ip_parts[2],
                    ip_parts[3],
                ),
                ntohs(sin.sin_port))
            )
        },
        AF_INET6 => {
            panic!("IPv6 not supported yet")
        },
        _ => {
            unreachable!("Should not happen")
        }
    }
}