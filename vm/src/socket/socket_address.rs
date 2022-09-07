use socket2::SockAddr;

#[cfg(unix)]
use {
    nix::sys::socket::sockaddr_un,
    std::ffi::OsStr,
    std::os::{raw::c_char, unix::ffi::OsStrExt},
};

#[cfg(unix)]
#[cfg_attr(feature = "cargo-clippy", allow(clippy::uninit_assumed_init))]
fn sun_path_offset() -> usize {
    use std::mem::MaybeUninit;

    let addr: sockaddr_un = unsafe { MaybeUninit::uninit().assume_init() };
    let base = &addr as *const _ as usize;
    let path = &addr.sun_path as *const _ as usize;

    path - base
}

#[cfg(unix)]
fn unix_socket_path(sockaddr: &SockAddr) -> String {
    let len = sockaddr.len() as usize - sun_path_offset();
    let raw_addr = unsafe { &*(sockaddr.as_ptr() as *mut sockaddr_un) };
    let path =
        unsafe { &*(&raw_addr.sun_path as *const [c_char] as *const [u8]) };

    if len == 0 || (cfg!(not(target_os = "linux")) && raw_addr.sun_path[0] == 0)
    {
        return String::new();
    }

    let (start, stop) =
        if raw_addr.sun_path[0] == 0 { (1, len) } else { (0, len - 1) };

    // Abstract names might contain NULL bytes and invalid UTF8. Since Inko
    // doesn't provide any better types at the moment we'll use a string and
    // convert the data to UTF8. A byte array would technically be better, but
    // these are mutable and make for an unpleasant runtime API.
    OsStr::from_bytes(&path[start..stop]).to_string_lossy().into_owned()
}

#[cfg(not(unix))]
fn unix_socket_path(_sockaddr: &SockAddr) -> String {
    String::new()
}

/// A wrapper around the system's structure for socket addresses, such as
/// `sockaddr_un` for UNIX sockets.
pub(crate) enum SocketAddress {
    /// A UNIX socket.
    ///
    /// We use a separate enum variant because datagram UNIX sockets will have
    /// the family field set to AF_UNSPEC when using certain functions such as
    /// recvfrom().
    Unix(SockAddr),

    /// A socket of another type, such as an IPv4 or IPv6 socket.
    Other(SockAddr),
}

impl SocketAddress {
    pub(crate) fn address(&self) -> Result<(String, i64), String> {
        match self {
            SocketAddress::Unix(sockaddr) => {
                Ok((unix_socket_path(sockaddr), 0))
            }
            SocketAddress::Other(sockaddr) => match sockaddr.as_socket() {
                Some(v) => Ok((v.ip().to_string(), i64::from(v.port()))),
                None => Err("The address family isn't supported".to_string()),
            },
        }
    }
}
