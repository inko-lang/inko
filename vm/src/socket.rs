pub mod socket_address;

use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::socket::socket_address::SocketAddress;
use crate::state::State;
use socket2::{Domain, SockAddr, Socket as RawSocket, Type};
use std::io;
use std::io::Read;
use std::mem::transmute;
use std::net::Shutdown;
use std::net::{IpAddr, SocketAddr};
use std::slice;
use std::sync::atomic::{AtomicI8, Ordering};
use std::time::Duration;

/// The registered value to use to signal a socket isn't registered with a
/// network poller.
const NOT_REGISTERED: i8 = -1;

#[cfg(unix)]
use libc::{EINPROGRESS, EISCONN};

#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock::{
    WSAEINPROGRESS as EINPROGRESS, WSAEISCONN as EISCONN,
};

macro_rules! socket_setter {
    ($setter:ident, $type:ty) => {
        pub(crate) fn $setter(&self, value: $type) -> Result<(), RuntimeError> {
            self.inner.$setter(value)?;

            Ok(())
        }
    };
}

macro_rules! socket_getter {
    ($getter:ident, $type:ty) => {
        pub(crate) fn $getter(&self) -> Result<$type, RuntimeError> {
            Ok(self.inner.$getter()?)
        }
    };
}

macro_rules! socket_u32_getter {
    ($getter:ident) => {
        pub(crate) fn $getter(&self) -> Result<usize, RuntimeError> {
            Ok(self.inner.$getter()? as usize)
        }
    };
}

macro_rules! socket_duration_setter {
    ($setter:ident) => {
        pub(crate) fn $setter(&self, value: u64) -> Result<(), RuntimeError> {
            let dur = Duration::from_nanos(value);

            self.inner.$setter(Some(dur))?;
            Ok(())
        }
    };
}

macro_rules! socket_duration_getter {
    ($getter:ident) => {
        pub(crate) fn $getter(&self) -> Result<i64, RuntimeError> {
            let dur = self.inner.$getter()?;

            Ok(dur.map(|v| v.as_nanos() as i64).unwrap_or(0))
        }
    };
}

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

fn update_buffer_length_and_capacity(buffer: &mut Vec<u8>, read: usize) {
    unsafe {
        buffer.set_len(buffer.len() + read);
    }

    buffer.shrink_to_fit();
}

fn socket_type(kind: i64) -> Result<Type, RuntimeError> {
    let kind = match kind {
        0 => Type::STREAM,
        1 => Type::DGRAM,
        2 => Type::SEQPACKET,
        3 => Type::RAW,
        _ => {
            return Err(RuntimeError::Panic(format!(
                "{} is not a valid socket type",
                kind
            )))
        }
    };

    Ok(kind)
}

/// A nonblocking socket that can be registered with a `NetworkPoller`.
pub(crate) struct Socket {
    /// The raw socket.
    inner: RawSocket,

    /// The ID of the network poller we're registered with.
    ///
    /// A value of -1 indicates the socket isn't registered with any poller.
    ///
    /// This flag is necessary because the system's polling mechanism may not
    /// allow overwriting existing registrations without setting some additional
    /// flags. For example, epoll requires the use of EPOLL_CTL_MOD when
    /// overwriting a registration, as using EPOLL_CTL_ADD will produce an error
    /// if a file descriptor is already registered.
    registered: AtomicI8,

    /// A flag indicating if we're dealing with a UNIX socket or not.
    unix: bool,
}

impl Socket {
    pub(crate) fn new(
        domain: Domain,
        kind: Type,
        unix: bool,
    ) -> Result<Self, RuntimeError> {
        let socket = RawSocket::new(domain, kind, None)?;

        socket.set_nonblocking(true)?;

        Ok(Socket {
            inner: socket,
            registered: AtomicI8::new(NOT_REGISTERED),
            unix,
        })
    }

    pub(crate) fn ipv4(kind_int: i64) -> Result<Socket, RuntimeError> {
        Self::new(Domain::IPV4, socket_type(kind_int)?, false)
    }

    pub(crate) fn ipv6(kind_int: i64) -> Result<Socket, RuntimeError> {
        Self::new(Domain::IPV6, socket_type(kind_int)?, false)
    }

    #[cfg(unix)]
    pub(crate) fn unix(kind_int: i64) -> Result<Socket, RuntimeError> {
        Self::new(Domain::UNIX, socket_type(kind_int)?, true)
    }

    #[cfg(not(unix))]
    pub(crate) fn unix(_: i64) -> Result<Socket, RuntimeError> {
        Err(RuntimeError::from(
            "UNIX sockets aren't supported on this platform",
        ))
    }

    pub(crate) fn bind(
        &self,
        address: &str,
        port: u16,
    ) -> Result<(), RuntimeError> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        self.inner.bind(&sockaddr)?;

        Ok(())
    }

    pub(crate) fn listen(&self, backlog: i32) -> Result<(), RuntimeError> {
        self.inner.listen(backlog)?;

        Ok(())
    }

    pub(crate) fn connect(
        &self,
        address: &str,
        port: u16,
    ) -> Result<(), RuntimeError> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        match self.inner.connect(&sockaddr) {
            Ok(_) => {}
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.raw_os_error() == Some(EINPROGRESS as i32) =>
            {
                if let Ok(Some(err)) = self.inner.take_error() {
                    // When performing a connect(), the error returned may be
                    // WouldBlock, with the actual error being stored in
                    // SO_ERROR on the socket. Windows in particular seems to
                    // take this approach.
                    return Err(err.into());
                }

                // On Windows a connect(2) might throw WSAEWOULDBLOCK, the
                // Windows equivalent of EAGAIN/EWOULDBLOCK. Other platforms may
                // also produce some error that Rust will report as WouldBlock.
                return Err(RuntimeError::WouldBlock);
            }
            Err(ref e) if e.raw_os_error() == Some(EISCONN as i32) => {
                // We may run into an EISCONN if a previous connect(2) attempt
                // would block. In this case we can just continue.
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        Ok(())
    }

    pub(crate) fn register(
        &mut self,
        state: &State,
        process: ProcessPointer,
        thread_poller_id: usize,
        interest: Interest,
    ) -> Result<(), RuntimeError> {
        let existing_id = self.registered.load(Ordering::Acquire);

        // Once registered, the process might be rescheduled immediately if
        // there is data available. This means that once we (re)register the
        // socket, it is not safe to use "self" anymore.
        //
        // To deal with this we:
        //
        // 1. Set "registered" _first_ (if necessary)
        // 2. Add the socket to the poller
        let result = if existing_id == NOT_REGISTERED {
            let poller = &state.network_pollers[thread_poller_id];

            self.registered.store(thread_poller_id as i8, Ordering::Release);
            poller.add(process, &self.inner, interest)
        } else {
            let poller = &state.network_pollers[existing_id as usize];

            poller.modify(process, &self.inner, interest)
        };

        // *DO NOT* use "self" from here on, as the socket/process may already
        // be running on a different thread.
        result.map_err(|e| e.into())
    }

    pub(crate) fn accept(&self) -> Result<Self, RuntimeError> {
        let (socket, _) = self.inner.accept()?;

        // Accepted sockets don't inherit the non-blocking status of the
        // listener, so we need to manually mark them as non-blocking.
        socket.set_nonblocking(true)?;

        Ok(Socket {
            inner: socket,
            registered: AtomicI8::new(NOT_REGISTERED),
            unix: self.unix,
        })
    }

    pub(crate) fn read(
        &self,
        buffer: &mut Vec<u8>,
        amount: usize,
    ) -> Result<usize, RuntimeError> {
        if amount > 0 {
            // We don't use take(), because that only terminates if:
            //
            // 1. We hit EOF, or
            // 2. We have read the desired number of bytes
            //
            // For files this is fine, but for sockets EOF is not triggered
            // until the socket is closed; which is almost always too late.
            let slice = socket_output_slice(buffer, amount);
            let read = self.inner.recv(unsafe { transmute(slice) })?;

            update_buffer_length_and_capacity(buffer, read);
            Ok(read)
        } else {
            Ok((&self.inner).read_to_end(buffer)?)
        }
    }

    pub(crate) fn recv_from(
        &self,
        buffer: &mut Vec<u8>,
        bytes: usize,
    ) -> Result<(String, i64), RuntimeError> {
        let slice = socket_output_slice(buffer, bytes);
        let (read, sockaddr) =
            self.inner.recv_from(unsafe { transmute(slice) })?;

        update_buffer_length_and_capacity(buffer, read);

        decode_sockaddr(sockaddr, self.unix)
    }

    pub(crate) fn send_to(
        &self,
        buffer: &[u8],
        address: &str,
        port: u16,
    ) -> Result<usize, RuntimeError> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        Ok(self.inner.send_to(buffer, &sockaddr)?)
    }

    pub(crate) fn local_address(&self) -> Result<(String, i64), RuntimeError> {
        let sockaddr = self.inner.local_addr()?;

        decode_sockaddr(sockaddr, self.unix)
    }

    pub(crate) fn peer_address(&self) -> Result<(String, i64), RuntimeError> {
        let sockaddr = self.inner.peer_addr()?;

        decode_sockaddr(sockaddr, self.unix)
    }

    pub(crate) fn shutdown_read(&self) -> Result<(), RuntimeError> {
        self.inner.shutdown(Shutdown::Read).map_err(|e| e.into())
    }

    pub(crate) fn shutdown_write(&self) -> Result<(), RuntimeError> {
        self.inner.shutdown(Shutdown::Write).map_err(|e| e.into())
    }

    pub(crate) fn shutdown_read_write(&self) -> Result<(), RuntimeError> {
        self.inner.shutdown(Shutdown::Both).map_err(|e| e.into())
    }

    socket_setter!(set_ttl, u32);
    socket_setter!(set_only_v6, bool);
    socket_setter!(set_nodelay, bool);
    socket_setter!(set_broadcast, bool);
    socket_setter!(set_reuse_address, bool);
    socket_setter!(set_keepalive, bool);

    socket_setter!(set_recv_buffer_size, usize);
    socket_setter!(set_send_buffer_size, usize);

    socket_duration_setter!(set_linger);

    socket_getter!(only_v6, bool);
    socket_getter!(nodelay, bool);
    socket_getter!(broadcast, bool);
    socket_getter!(reuse_address, bool);
    socket_getter!(keepalive, bool);

    socket_getter!(recv_buffer_size, usize);
    socket_getter!(send_buffer_size, usize);

    socket_u32_getter!(ttl);

    socket_duration_getter!(linger);

    #[cfg(unix)]
    pub(crate) fn set_reuse_port(
        &self,
        reuse: bool,
    ) -> Result<(), RuntimeError> {
        Ok(self.inner.set_reuse_port(reuse)?)
    }

    #[cfg(not(unix))]
    pub(crate) fn set_reuse_port(
        &self,
        _reuse: bool,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn reuse_port(&self) -> Result<bool, RuntimeError> {
        Ok(self.inner.reuse_port()?)
    }

    #[cfg(not(unix))]
    pub(crate) fn reuse_port(&self) -> Result<bool, RuntimeError> {
        Ok(false)
    }

    pub(crate) fn try_clone(&self) -> Result<Socket, RuntimeError> {
        let sock = Socket {
            inner: self.inner.try_clone()?,
            registered: AtomicI8::new(NOT_REGISTERED),
            unix: self.unix,
        };

        Ok(sock)
    }
}

impl io::Write for Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl io::Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.recv(unsafe { transmute(buf) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_clone() {
        let socket1 = Socket::ipv4(0).unwrap();

        socket1.registered.store(2, Ordering::Release);

        let socket2 = socket1.try_clone().unwrap();

        assert_eq!(socket2.registered.load(Ordering::Acquire), NOT_REGISTERED);
        assert!(!socket2.unix);
    }
}
