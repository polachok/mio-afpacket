extern crate libc;
extern crate mio;

use mio::event::Evented;
use mio::unix::EventedFd;
use mio::{Poll, PollOpt, Ready, Token};
use std::io::{Error, Read, Result};
use std::os::unix::io::{RawFd, IntoRawFd, AsRawFd, FromRawFd};

use libc::socket;
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
}

impl Read for RawPacketStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let rv = unsafe { libc::read(self.0, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if rv < 0 {
            return Err(Error::last_os_error());
        }

        Ok(rv as usize)
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
    fn it_debugs() {
        let raw = RawPacketStream::new().unwrap();
        println!("{:?}", raw);
    }
}
