pub mod socket_address;

use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::socket::socket_address::SocketAddress;
use crate::state::State;
use rustix::io::Errno;
use socket2::{Domain, SockAddr, Socket as RawSocket, Type};
use std::io::{self, Read};
use std::mem::transmute;
use std::net::Shutdown;
use std::net::{IpAddr, SocketAddr};
use std::slice;
use std::sync::atomic::{AtomicI8, Ordering};
use std::time::Duration;

/// The registered value to use to signal a socket isn't registered with a
/// network poller.
const NOT_REGISTERED: i8 = -1;

macro_rules! socket_setter {
    ($setter:ident, $type:ty) => {
        pub(crate) fn $setter(&self, value: $type) -> io::Result<()> {
            self.inner.$setter(value)
        }
    };
}

macro_rules! socket_duration_setter {
    ($setter:ident) => {
        pub(crate) fn $setter(&self, value: u64) -> io::Result<()> {
            self.inner.$setter(Some(Duration::from_nanos(value)))
        }
    };
}

/// Decodes a SockAddr into an address/path, and a port.
fn decode_sockaddr(
    sockaddr: SockAddr,
    unix: bool,
) -> Result<(String, i64), String> {
    if unix {
        SocketAddress::Unix(sockaddr).address()
    } else {
        SocketAddress::Other(sockaddr).address()
    }
}

#[cfg(unix)]
fn encode_sockaddr(
    address: &str,
    port: u16,
    unix: bool,
) -> io::Result<SockAddr> {
    if unix {
        return SockAddr::unix(address);
    }

    address
        .parse::<IpAddr>()
        .map(|ip| SockAddr::from(SocketAddr::new(ip, port)))
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}

#[cfg(not(unix))]
fn encode_sockaddr(
    address: &str,
    port: u16,
    _unix: bool,
) -> io::Result<SockAddr> {
    address
        .parse::<IpAddr>()
        .map(|ip| SockAddr::from(SocketAddr::new(ip, port)))
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
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

fn socket_type(kind: i64) -> io::Result<Type> {
    match kind {
        0 => Ok(Type::STREAM),
        1 => Ok(Type::DGRAM),
        2 => Ok(Type::SEQPACKET),
        3 => Ok(Type::RAW),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("{} is not a valid socket type", kind),
        )),
    }
}

/// A nonblocking socket that can be registered with a `NetworkPoller`.
///
/// When changing the layout of this type, don't forget to also update its
/// definition in the standard library.
#[repr(C)]
pub struct Socket {
    /// The raw socket.
    pub inner: RawSocket,

    /// The ID of the network poller we're registered with.
    ///
    /// A value of -1 indicates the socket isn't registered with any poller.
    ///
    /// This flag is necessary because the system's polling mechanism may not
    /// allow overwriting existing registrations without setting some additional
    /// flags. For example, epoll requires the use of EPOLL_CTL_MOD when
    /// overwriting a registration, as using EPOLL_CTL_ADD will produce an error
    /// if a file descriptor is already registered.
    pub registered: AtomicI8,

    /// A flag indicating if we're dealing with a UNIX socket or not.
    pub unix: bool,
}

impl Socket {
    pub(crate) fn new(
        domain: Domain,
        kind: Type,
        unix: bool,
    ) -> io::Result<Self> {
        let socket = RawSocket::new(domain, kind, None)?;

        socket.set_nonblocking(true)?;

        Ok(Socket {
            inner: socket,
            registered: AtomicI8::new(NOT_REGISTERED),
            unix,
        })
    }

    pub(crate) fn ipv4(kind_int: i64) -> io::Result<Socket> {
        Self::new(Domain::IPV4, socket_type(kind_int)?, false)
    }

    pub(crate) fn ipv6(kind_int: i64) -> io::Result<Socket> {
        Self::new(Domain::IPV6, socket_type(kind_int)?, false)
    }

    #[cfg(unix)]
    pub(crate) fn unix(kind_int: i64) -> io::Result<Socket> {
        Self::new(Domain::UNIX, socket_type(kind_int)?, true)
    }

    #[cfg(not(unix))]
    pub(crate) fn unix(_: i64) -> io::Result<Socket> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "UNIX sockets aren't supported on this platform",
        ))
    }

    pub(crate) fn bind(&self, address: &str, port: u16) -> io::Result<()> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        self.inner.bind(&sockaddr)
    }

    pub(crate) fn listen(&self, backlog: i32) -> io::Result<()> {
        self.inner.listen(backlog)
    }

    pub(crate) fn connect(&self, address: &str, port: u16) -> io::Result<()> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        match self.inner.connect(&sockaddr) {
            Ok(_) => Ok(()),
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.raw_os_error()
                        == Some(Errno::INPROGRESS.raw_os_error()) =>
            {
                if let Ok(Some(err)) = self.inner.take_error() {
                    // When performing a connect(), the error returned may be
                    // WouldBlock, with the actual error being stored in
                    // SO_ERROR on the socket.
                    return Err(err);
                }

                Err(io::Error::from(io::ErrorKind::WouldBlock))
            }
            Err(ref e)
                if e.raw_os_error() == Some(Errno::ISCONN.raw_os_error()) =>
            {
                // We may run into an ISCONN if a previous connect(2) attempt
                // would block. In this case we can just continue.
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub(crate) fn register(
        &mut self,
        state: &State,
        process: ProcessPointer,
        thread_poller_id: usize,
        interest: Interest,
    ) -> io::Result<()> {
        let existing_id = self.registered.load(Ordering::Acquire);

        // Once registered, the process might be rescheduled immediately if
        // there is data available. This means that once we (re)register the
        // socket, it is not safe to use "self" anymore.
        //
        // To deal with this we:
        //
        // 1. Set "registered" _first_ (if necessary)
        // 2. Add the socket to the poller
        if existing_id == NOT_REGISTERED {
            let poller = &state.network_pollers[thread_poller_id];

            self.registered.store(thread_poller_id as i8, Ordering::Release);

            poller.add(process, &self.inner, interest)
        } else {
            let poller = &state.network_pollers[existing_id as usize];

            poller.modify(process, &self.inner, interest)
        }
        // *DO NOT* use "self" from here on, as the socket/process may already
        // be running on a different thread.
    }

    pub(crate) fn deregister(&mut self, state: &State) {
        let poller_id = self.registered.load(Ordering::Acquire) as usize;
        let _ = state.network_pollers[poller_id].delete(&self.inner);
    }

    pub(crate) fn accept(&self) -> io::Result<Self> {
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
    ) -> io::Result<usize> {
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
    ) -> io::Result<(String, i64)> {
        let slice = socket_output_slice(buffer, bytes);
        let (read, sockaddr) =
            self.inner.recv_from(unsafe { transmute(slice) })?;

        update_buffer_length_and_capacity(buffer, read);

        decode_sockaddr(sockaddr, self.unix)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    pub(crate) fn send_to(
        &self,
        buffer: &[u8],
        address: &str,
        port: u16,
    ) -> io::Result<usize> {
        let sockaddr = encode_sockaddr(address, port, self.unix)?;

        self.inner.send_to(buffer, &sockaddr)
    }

    pub(crate) fn local_address(&self) -> io::Result<(String, i64)> {
        let sockaddr = self.inner.local_addr()?;

        decode_sockaddr(sockaddr, self.unix)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    pub(crate) fn peer_address(&self) -> io::Result<(String, i64)> {
        let sockaddr = self.inner.peer_addr()?;

        decode_sockaddr(sockaddr, self.unix)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    pub(crate) fn shutdown_read(&self) -> io::Result<()> {
        self.inner.shutdown(Shutdown::Read)
    }

    pub(crate) fn shutdown_write(&self) -> io::Result<()> {
        self.inner.shutdown(Shutdown::Write)
    }

    pub(crate) fn shutdown_read_write(&self) -> io::Result<()> {
        self.inner.shutdown(Shutdown::Both)
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

    #[cfg(unix)]
    pub(crate) fn set_reuse_port(&self, reuse: bool) -> io::Result<()> {
        self.inner.set_reuse_port(reuse)
    }

    #[cfg(not(unix))]
    pub(crate) fn set_reuse_port(&self, _reuse: bool) -> io::Result<()> {
        Ok(())
    }

    pub(crate) fn try_clone(&self) -> io::Result<Socket> {
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
    use std::mem::size_of;

    #[test]
    fn test_try_clone() {
        let socket1 = Socket::ipv4(0).unwrap();

        socket1.registered.store(2, Ordering::Release);

        let socket2 = socket1.try_clone().unwrap();

        assert_eq!(socket2.registered.load(Ordering::Acquire), NOT_REGISTERED);
        assert!(!socket2.unix);
    }

    #[test]
    fn test_type_size() {
        assert_eq!(size_of::<Socket>(), 8);
    }
}
