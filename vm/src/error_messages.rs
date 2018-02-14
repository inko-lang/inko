use std::error::Error;
use std::io;

pub const IO_PERMISSION_DENIED: &'static str =
    "The operation lacked the necessary privileges to complete.";

pub const IO_CONNECTION_REFUSED: &'static str =
    "The connection was refused by the remote server.";

pub const IO_CONNECTION_RESET: &'static str =
    "The connection was reset by the remote server.";

pub const IO_CONNECTION_ABORTED: &'static str =
    "The connection was terminated by the remote server.";

pub const IO_NOT_CONNECTED: &'static str =
    "The operation failed because the connection has not yet been established.";

pub const IO_ADDRESS_IN_USE: &'static str = "The address is already in use.";

pub const IO_ADDRESS_NOT_AVAILABLE: &'static str =
    "The address is not available.";

pub const IO_BROKEN_PIPE: &'static str =
    "The operation failed because a pipe was closed.";

pub const IO_ALREADY_EXISTS: &'static str = "The resource already exists.";

pub const IO_WOULD_BLOCK: &'static str =
    "The operation failed as it would block.";

pub const IO_INVALID_INPUT: &'static str = "An input parameter is invalid.";

pub const IO_INVALID_DATA: &'static str =
    "The supplied data is not valid for this operation.";

pub const IO_TIMED_OUT: &'static str = "The operation timed out.";

pub const IO_WRITE_ZERO: &'static str =
    "The operation failed because not enough bytes were written.";

pub const IO_INTERRUPTED: &'static str = "The operation was interrupted.";

pub const IO_UNEXPECTED_EOF: &'static str =
    "The operation failed because of an unexpected end-of-file.";

pub const IO_NOT_FOUND: &'static str = "The resource could not be found.";

/// Returns an error message from a Rust IO error.
pub fn from_io_error(error: io::Error) -> String {
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
