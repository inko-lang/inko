use std::error::Error;
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
    let slice = match error.kind() {
        io::ErrorKind::NotFound => IO_NOT_FOUND,
        io::ErrorKind::PermissionDenied => IO_PERMISSION_DENIED,
        io::ErrorKind::ConnectionRefused => IO_CONNECTION_REFUSED,
        io::ErrorKind::ConnectionReset => IO_CONNECTION_RESET,
        io::ErrorKind::ConnectionAborted => IO_CONNECTION_ABORTED,
        io::ErrorKind::NotConnected => IO_NOT_CONNECTED,
        io::ErrorKind::AddrInUse => IO_ADDRESS_IN_USE,
        io::ErrorKind::AddrNotAvailable => IO_ADDRESS_NOT_AVAILABLE,
        io::ErrorKind::BrokenPipe => IO_BROKEN_PIPE,
        io::ErrorKind::AlreadyExists => IO_ALREADY_EXISTS,
        io::ErrorKind::WouldBlock => IO_WOULD_BLOCK,
        io::ErrorKind::InvalidInput => IO_INVALID_INPUT,
        io::ErrorKind::InvalidData => IO_INVALID_DATA,
        io::ErrorKind::TimedOut => IO_TIMED_OUT,
        io::ErrorKind::WriteZero => IO_WRITE_ZERO,
        io::ErrorKind::Interrupted => IO_INTERRUPTED,
        io::ErrorKind::UnexpectedEof => IO_UNEXPECTED_EOF,
        _ => error.description(),
    };

    slice.to_string()
}
