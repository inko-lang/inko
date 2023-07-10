use socket2::SockAddr;

#[cfg(unix)]
use {
    rustix::net::SocketAddrUnix, std::ffi::OsStr, std::os::unix::ffi::OsStrExt,
};

#[cfg(unix)]
fn unix_socket_path(sockaddr: &SockAddr) -> String {
    sockaddr
        .as_pathname()
        .and_then(|p| SocketAddrUnix::new(p).ok())
        .and_then(|addr| {
            addr.path().map(|path| {
                // Abstract names might contain NULL bytes and invalid UTF8.
                // Since Inko doesn't provide any better types at the moment
                // we'll use a string and convert the data to UTF8. A byte array
                // would technically be better, but these are mutable and make
                // for an unpleasant runtime API.
                OsStr::from_bytes(path.to_bytes())
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn test_unix_socket_path() {
        let path1 = unix_socket_path(&SockAddr::unix("foo.sock").unwrap());
        let path2 = unix_socket_path(&SockAddr::unix("").unwrap());
        let path3 = unix_socket_path(&SockAddr::unix("\0").unwrap());

        assert_eq!(path1, "foo.sock".to_string());
        assert_eq!(path2, String::new());
        assert_eq!(path3, String::new());
    }
}
