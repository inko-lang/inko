pub mod socket_address;

use crate::arc_without_weak::ArcWithoutWeak;
use crate::duration;
use crate::network_poller::event_id::EventId;
use crate::network_poller::interest::Interest;
use crate::network_poller::NetworkPoller;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::socket::socket_address::SocketAddress;
use socket2::{Domain, SockAddr, Socket as RawSocket, Type};
use std::io;
use std::net::Ipv4Addr;
use std::net::Shutdown;
use std::net::{IpAddr, SocketAddr};
use std::slice;

#[cfg(unix)]
use libc::EINPROGRESS;

#[cfg(windows)]
use winapi::shared::winerror::WSAEINPROGRESS as EINPROGRESS;

macro_rules! socket_setter {
    ($setter:ident, $type:ty) => {
        pub fn $setter(&self, value: $type) -> Result<(), RuntimeError> {
            self.socket.$setter(value)?;

            Ok(())
        }
    }
}

macro_rules! socket_getter {
    ($getter:ident, $type:ty) => {
        pub fn $getter(&self) -> Result<$type, RuntimeError> {
            Ok(self.socket.$getter()?)
        }
    }
}

macro_rules! socket_u32_getter {
    ($getter:ident) => {
        pub fn $getter(&self) -> Result<usize, RuntimeError> {
            Ok(self.socket.$getter()? as usize)
        }
    }
}

macro_rules! socket_duration_setter {
    ($setter:ident) => {
        pub fn $setter(&self, value: f64) -> Result<(), RuntimeError> {
            let dur = duration::from_f64(value)?;

            self.socket.$setter(dur)?;

            Ok(())
        }
    }
}

macro_rules! socket_duration_getter {
    ($getter:ident) => {
        pub fn $getter(&self) -> Result<f64, RuntimeError> {
            let dur = self.socket.$getter()?;

            Ok(duration::to_f64(dur))
        }
    }
}

const DOMAIN_IPV4: u8 = 0;
const DOMAIN_IPV6: u8 = 1;
const DOMAIN_UNIX: u8 = 2;

/// Decodes a SockAddr into an address/path, and a port.
fn decode_sockaddr(
    sockaddr: SockAddr,
    unix: bool,
) -> Result<(String, i64), RuntimeError> {
    let peer_result = if unix {
        SocketAddress::Unix(sockaddr).address()
    } else {
        SocketAddress::Other(sockaddr).address()
    };

    Ok(peer_result?)
}

#[cfg(unix)]
fn encode_sockaddr(
    address: &str,
    port: u16,
    unix: bool,
) -> Result<SockAddr, RuntimeError> {
    if unix {
        return Ok(SockAddr::unix(address)?);
    }

    let ip = address.parse::<IpAddr>()?;

    Ok(SockAddr::from(SocketAddr::new(ip, port)))
}

#[cfg(not(unix))]
fn encode_sockaddr(
    address: &str,
    port: u16,
    _unix: bool,
) -> Result<SockAddr, RuntimeError> {
    let ip = address.parse::<IpAddr>()?;

    Ok(SockAddr::from(SocketAddr::new(ip, port)))
}

/// Returns a slice of the input buffer that a socket operation can write to.
///
/// The slice has enough space to store up to `bytes` of data.
fn socket_output_slice(buffer: &mut Vec<u8>, bytes: usize) -> &mut [u8] {
    let len = buffer.len();
    let available = buffer.capacity() - len;
    let to_reserve = bytes - available;

    if to_reserve > 0 {
        // Only increasing capacity when needed is done for two reasons:
        //
        // 1. It saves us from increasing capacity when there is enough space.
        //
        // 2. Due to sockets being non-blocking, a socket operation may fail.
        //    This will result in this code being called multiple times. If we
        //    were to simply increase capacity every time we'd end up growing
        //    the buffer much more than necessary.
        buffer.reserve_exact(to_reserve);
    }

    unsafe { slice::from_raw_parts_mut(buffer.as_mut_ptr().add(len), bytes) }
}

/// A nonblocking socket that can be registered with a `NetworkPoller`.
pub struct Socket {
    /// The raw socket.
    socket: RawSocket,

    /// A flag indicating that this socket has been registered with a poller.
    ///
    /// This flag is necessary because the system's polling mechanism may not
    /// allow overwriting existing registrations without setting some additional
    /// flags. For example, epoll requires the use of EPOLL_CTL_MOD when
    /// overwriting a registration, as using EPOLL_CTL_ADD will produce an error
    /// if a file descriptor is already registered.
    registered: bool,

    /// A flag indicating if we're dealing with a UNIX socket or not.
    unix: bool,
}

impl Socket {
    pub fn new(domain_int: u8, kind_int: u8) -> Result<Socket, RuntimeError> {
        let domain = match domain_int {
            DOMAIN_IPV4 => Domain::ipv4(),
            DOMAIN_IPV6 => Domain::ipv6(),

            #[cfg(unix)]
            DOMAIN_UNIX => Domain::unix(),

            _ => {
                return Err(RuntimeError::Panic(format!(
                    "{} is not a valid socket domain",
                    domain_int
                )))
            }
        };

        let kind = match kind_int {
            0 => Type::stream(),
            1 => Type::dgram(),
            2 => Type::seqpacket(),
            3 => Type::raw(),
            _ => {
                return Err(RuntimeError::Panic(format!(
                    "{} is not a valid socket type",
                    kind_int
                )))
            }
        };

        let socket = RawSocket::new(domain, kind, None)?;

        socket.set_nonblocking(true)?;

        Ok(Socket {
            socket,
            registered: false,
            unix: domain_int == DOMAIN_UNIX,
        })
    }

    pub fn bind(&self, address: &str, port: u16) -> Result<(), RuntimeError> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        self.socket.bind(&sockaddr)?;

        Ok(())
    }

    pub fn listen(&self, backlog: i32) -> Result<(), RuntimeError> {
        self.socket.listen(backlog)?;

        Ok(())
    }

    pub fn connect(
        &self,
        address: &str,
        port: u16,
    ) -> Result<(), RuntimeError> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        match self.socket.connect(&sockaddr) {
            Ok(_) => {}
            #[cfg(windows)]
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // On Windows a connect(2) might throw WSAEWOULDBLOCK, the
                // Windows equivalent of EAGAIN/EWOULDBLOCK. When this happens
                // we should not retry the connect(), as that may then fail with
                // WSAEISCONN. Instead, we signal that the network poller should
                // just wait until the socket is ready for writing.
                return Err(RuntimeError::InProgress);
            }
            Err(ref e) if e.raw_os_error() == Some(EINPROGRESS as i32) => {
                return Err(RuntimeError::InProgress);
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        Ok(())
    }

    pub fn register(
        &mut self,
        process: &RcProcess,
        poller: &NetworkPoller,
        interest: Interest,
    ) -> Result<(), RuntimeError> {
        let event_id =
            EventId(ArcWithoutWeak::into_raw(process.clone()) as u64);

        if self.registered {
            poller.reregister(&self.socket, event_id, interest)?;
        } else {
            poller.register(&self.socket, event_id, interest)?;

            self.registered = true;
        }

        Ok(())
    }

    pub fn accept(&self) -> Result<Self, RuntimeError> {
        let (socket, _) = self.socket.accept()?;

        // Accepted sockets don't inherit the non-blocking status of the
        // listener, so we need to manually mark them as non-blocking.
        socket.set_nonblocking(true)?;

        Ok(Socket {
            socket,
            registered: false,
            unix: self.unix,
        })
    }

    pub fn recv_from(
        &self,
        buffer: &mut Vec<u8>,
        bytes: usize,
    ) -> Result<(String, i64), RuntimeError> {
        let slice = socket_output_slice(buffer, bytes);
        let (read, sockaddr) = self.socket.recv_from(slice)?;

        unsafe {
            buffer.set_len(buffer.len() + read);
        }

        Ok(decode_sockaddr(sockaddr, self.unix)?)
    }

    pub fn send_to(
        &self,
        buffer: &[u8],
        address: &str,
        port: u16,
    ) -> Result<usize, RuntimeError> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        Ok(self.socket.send_to(buffer, &sockaddr)?)
    }

    pub fn local_address(&self) -> Result<(String, i64), RuntimeError> {
        let sockaddr = self.socket.local_addr()?;

        Ok(decode_sockaddr(sockaddr, self.unix)?)
    }

    pub fn peer_address(&self) -> Result<(String, i64), RuntimeError> {
        let sockaddr = self.socket.peer_addr()?;

        Ok(decode_sockaddr(sockaddr, self.unix)?)
    }

    pub fn is_unix(&self) -> bool {
        self.unix
    }

    pub fn shutdown(&self, mode: u8) -> Result<(), RuntimeError> {
        let shutdown = match mode {
            0 => Shutdown::Read,
            1 => Shutdown::Write,
            2 => Shutdown::Both,
            _ => {
                return Err(RuntimeError::Panic(format!(
                    "{} is not a valid mode to shut down",
                    mode
                )));
            }
        };

        Ok(self.socket.shutdown(shutdown)?)
    }

    socket_setter!(set_ttl, u32);
    socket_setter!(set_only_v6, bool);
    socket_setter!(set_nodelay, bool);
    socket_setter!(set_broadcast, bool);
    socket_setter!(set_multicast_loop_v4, bool);
    socket_setter!(set_multicast_loop_v6, bool);
    socket_setter!(set_reuse_address, bool);

    socket_setter!(set_recv_buffer_size, usize);
    socket_setter!(set_send_buffer_size, usize);
    socket_setter!(set_multicast_ttl_v4, u32);
    socket_setter!(set_multicast_hops_v6, u32);
    socket_setter!(set_multicast_if_v6, u32);
    socket_setter!(set_unicast_hops_v6, u32);

    socket_duration_setter!(set_linger);
    socket_duration_setter!(set_keepalive);

    socket_getter!(only_v6, bool);
    socket_getter!(nodelay, bool);
    socket_getter!(broadcast, bool);
    socket_getter!(multicast_loop_v4, bool);
    socket_getter!(multicast_loop_v6, bool);
    socket_getter!(reuse_address, bool);

    socket_getter!(recv_buffer_size, usize);
    socket_getter!(send_buffer_size, usize);

    socket_u32_getter!(ttl);
    socket_u32_getter!(multicast_ttl_v4);
    socket_u32_getter!(multicast_hops_v6);
    socket_u32_getter!(multicast_if_v6);
    socket_u32_getter!(unicast_hops_v6);

    socket_duration_getter!(linger);
    socket_duration_getter!(keepalive);

    pub fn set_multicast_if_v4(&self, addr: &str) -> Result<(), RuntimeError> {
        let ip_addr = addr.parse::<Ipv4Addr>()?;

        Ok(self.socket.set_multicast_if_v4(&ip_addr)?)
    }

    pub fn multicast_if_v4(&self) -> Result<String, RuntimeError> {
        Ok(self.socket.multicast_if_v4().map(|addr| addr.to_string())?)
    }

    #[cfg(unix)]
    pub fn set_reuse_port(&self, reuse: bool) -> Result<(), RuntimeError> {
        Ok(self.socket.set_reuse_port(reuse)?)
    }

    #[cfg(not(unix))]
    pub fn set_reuse_port(&self, _reuse: bool) -> Result<(), RuntimeError> {
        Ok(())
    }

    #[cfg(unix)]
    pub fn reuse_port(&self) -> Result<bool, RuntimeError> {
        Ok(self.socket.reuse_port()?)
    }

    #[cfg(not(unix))]
    pub fn reuse_port(&self) -> Result<bool, RuntimeError> {
        Ok(false)
    }
}

impl io::Write for Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.socket.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}

impl io::Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.recv(buf)
    }
}

impl Clone for Socket {
    fn clone(&self) -> Self {
        Socket {
            socket: self
                .socket
                .try_clone()
                .expect("Failed to clone the socket"),
            registered: self.registered,
            unix: self.unix,
        }
    }
}
