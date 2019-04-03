use std::io;

pub const IO_PERMISSION_DENIED: &str =
    "The operation lacked the necessary privileges to complete.";

pub const IO_CONNECTION_REFUSED: &str =
    "The connection was refused by the remote server.";

pub const IO_CONNECTION_RESET: &str =
    "The connection was reset by the remote server.";

pub const IO_CONNECTION_ABORTED: &str =
    "The connection was terminated by the remote server.";

pub const IO_NOT_CONNECTED: &str =
    "The operation failed because the connection has not yet been established.";

pub const IO_ADDRESS_IN_USE: &str = "The address is already in use.";

pub const IO_ADDRESS_NOT_AVAILABLE: &str = "The address is not available.";

pub const IO_BROKEN_PIPE: &str =
    "The operation failed because a pipe was closed.";

pub const IO_ALREADY_EXISTS: &str = "The resource already exists.";

pub const IO_WOULD_BLOCK: &str = "The operation failed as it would block.";

pub const IO_INVALID_INPUT: &str = "An input parameter is invalid.";

pub const IO_INVALID_DATA: &str =
    "The supplied data is not valid for this operation.";

pub const IO_TIMED_OUT: &str = "The operation timed out.";

pub const IO_WRITE_ZERO: &str =
    "The operation failed because not enough bytes were written.";

pub const IO_INTERRUPTED: &str = "The operation was interrupted.";

pub const IO_UNEXPECTED_EOF: &str =
    "The operation failed because of an unexpected end-of-file.";

pub const IO_NOT_FOUND: &str = "The resource could not be found.";

/// Returns an error message from a Rust IO error.
pub fn from_io_error(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => IO_NOT_FOUND.to_string(),
        io::ErrorKind::PermissionDenied => IO_PERMISSION_DENIED.to_string(),
        io::ErrorKind::ConnectionRefused => IO_CONNECTION_REFUSED.to_string(),
        io::ErrorKind::ConnectionReset => IO_CONNECTION_RESET.to_string(),
        io::ErrorKind::ConnectionAborted => IO_CONNECTION_ABORTED.to_string(),
        io::ErrorKind::NotConnected => IO_NOT_CONNECTED.to_string(),
        io::ErrorKind::AddrInUse => IO_ADDRESS_IN_USE.to_string(),
        io::ErrorKind::AddrNotAvailable => IO_ADDRESS_NOT_AVAILABLE.to_string(),
        io::ErrorKind::BrokenPipe => IO_BROKEN_PIPE.to_string(),
        io::ErrorKind::AlreadyExists => IO_ALREADY_EXISTS.to_string(),
        io::ErrorKind::WouldBlock => IO_WOULD_BLOCK.to_string(),
        io::ErrorKind::InvalidInput => IO_INVALID_INPUT.to_string(),
        io::ErrorKind::InvalidData => IO_INVALID_DATA.to_string(),
        io::ErrorKind::TimedOut => IO_TIMED_OUT.to_string(),
        io::ErrorKind::WriteZero => IO_WRITE_ZERO.to_string(),
        io::ErrorKind::Interrupted => IO_INTERRUPTED.to_string(),
        io::ErrorKind::UnexpectedEof => IO_UNEXPECTED_EOF.to_string(),
        _ => error.to_string(),
    }
}
