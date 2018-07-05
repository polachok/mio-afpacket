extern crate libc;
extern crate mio;

use mio::event::Evented;
use mio::unix::EventedFd;
use mio::{Poll, PollOpt, Ready, Token};
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

use libc::{sockaddr_ll, sockaddr_storage, socket};
use libc::{AF_PACKET, ETH_P_ALL, SOCK_RAW};

/// Packet sockets are used to receive or send raw packets at OSI 2 level.
#[derive(Debug)]
pub struct RawPacketStream(RawFd);

impl Evented for RawPacketStream {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        poll.register(&EventedFd(&self.0), token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        poll.reregister(&EventedFd(&self.0), token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> Result<()> {
        poll.deregister(self)
    }
}

impl RawPacketStream {
    /// Create new raw packet stream binding to all interfaces
    pub fn new() -> Result<Self> {
        let fd = unsafe { socket(AF_PACKET, SOCK_RAW, (ETH_P_ALL as u16).to_be() as i32) };
        if fd == -1 {
            return Err(Error::last_os_error());
        }
        let mut nonblocking = 1 as libc::c_ulong;
        let res = unsafe { libc::ioctl(fd, libc::FIONBIO, &mut nonblocking) };

        if res == -1 {
            return Err(Error::last_os_error());
        }
        Ok(RawPacketStream(fd as RawFd))
    }

    /// Bind socket to an interface (by name).
    pub fn bind(&mut self, name: &str) -> Result<()> {
        if name.len() > libc::IFNAMSIZ {
            return Err(ErrorKind::InvalidInput.into());
        }
        let mut buf = [0u8; libc::IFNAMSIZ];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        let idx = unsafe { libc::if_nametoindex(buf.as_ptr() as *const libc::c_char) };
        if idx == 0 {
            return Err(Error::last_os_error());
        }
        self.bind_by_index(idx as i32)
    }

    fn bind_by_index(&mut self, ifindex: i32) -> Result<()> {
        unsafe {
            let mut ss: sockaddr_storage = std::mem::zeroed();
            let sll: *mut sockaddr_ll = std::mem::transmute(&mut ss);
            (*sll).sll_family = AF_PACKET as u16;
            (*sll).sll_protocol = (ETH_P_ALL as u16).to_be();
            (*sll).sll_ifindex = ifindex;

            let sa = (&ss as *const libc::sockaddr_storage) as *const libc::sockaddr;
            let res = libc::bind(self.0, sa, std::mem::size_of::<sockaddr_ll>() as u32);
            if res == -1 {
                return Err(Error::last_os_error());
            }
        }
        Ok(())
    }
}

fn read_fd(fd: RawFd, buf: &mut [u8]) -> Result<usize> {
    let rv = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
    if rv < 0 {
        return Err(Error::last_os_error());
    }

    Ok(rv as usize)
}

impl Read for RawPacketStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        read_fd(self.0, buf)
    }
}

impl<'a> Read for &'a RawPacketStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        read_fd(self.0, buf)
    }
}

fn write_fd(fd: RawFd, buf: &[u8]) -> Result<usize> {
    let rv = unsafe { libc::write(fd, buf.as_ptr() as *const libc::c_void, buf.len()) };
    if rv < 0 {
        return Err(Error::last_os_error());
    }

    Ok(rv as usize)
}

impl Write for RawPacketStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        write_fd(self.0, buf)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<'a> Write for &'a RawPacketStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        write_fd(self.0, buf)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl IntoRawFd for RawPacketStream {
    fn into_raw_fd(self) -> RawFd {
        self.0
    }
}

impl AsRawFd for RawPacketStream {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl FromRawFd for RawPacketStream {
    unsafe fn from_raw_fd(fd: RawFd) -> RawPacketStream {
        RawPacketStream(fd)
    }
}

// tests require CAP_NET_RAW capabilities
#[cfg(test)]
mod tests {
    use RawPacketStream;

    #[test]
    fn it_works() {
        RawPacketStream::new().unwrap();
    }

    #[test]
    fn it_receives_something() {
        use mio::*;
        use std::io::Read;

        let mut raw = RawPacketStream::new().unwrap();
        let token = Token(0);
        let poll = Poll::new().unwrap();
        poll.register(&raw, token, Ready::readable(), PollOpt::edge())
            .unwrap();
        let mut events = Events::with_capacity(1024);
        loop {
            poll.poll(&mut events, None).unwrap();

            for _event in &events {
                let mut buf = [0; 1024];
                if let Ok(len) = raw.read(&mut buf) {
                    println!("pkt: {:02x?}", &buf[..len]);
                }
            }
        }
    }

    #[test]
    fn bind_works() {
        use mio::*;
        use std::io::Read;

        let mut raw = RawPacketStream::new().unwrap();
        raw.bind("lo").unwrap();

        let token = Token(0);
        let poll = Poll::new().unwrap();
        poll.register(&raw, token, Ready::readable(), PollOpt::edge())
            .unwrap();
        let mut events = Events::with_capacity(1024);
        loop {
            poll.poll(&mut events, None).unwrap();

            for _event in &events {
                let mut buf = [0; 1024];
                if let Ok(len) = raw.read(&mut buf) {
                    println!("pkt: {:02x?}", &buf[..len]);
                }
            }
        }
    }

    #[test]
    fn write_works() {
        use mio::*;
        use std::io::Read;

        let mut raw = RawPacketStream::new().unwrap();
        raw.bind("lo").unwrap();

        let token = Token(0);
        let poll = Poll::new().unwrap();
        poll.register(
            &raw,
            token,
            Ready::readable() | Ready::writable(),
            PollOpt::edge(),
        ).unwrap();
        let mut events = Events::with_capacity(1024);
        loop {
            poll.poll(&mut events, None).unwrap();

            for _event in &events {
                println!("{:?}", _event);
                let mut buf = [0; 1024];
                if let Ok(len) = raw.read(&mut buf) {
                    println!("pkt: {:02x?}", &buf[..len]);
                }
            }
        }
    }

    #[test]
    fn it_debugs() {
        let raw = RawPacketStream::new().unwrap();
        println!("{:?}", raw);
    }
}
