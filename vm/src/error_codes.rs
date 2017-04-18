use std::io;

// Generic errors

pub const STRING_INVALID_UTF8: u16 = 1;

// IO errors, ranging from 100 to 200.

pub const IO_GENERIC: u16 = 100;
pub const IO_PERMISSION_DENIED: u16 = 101;
pub const IO_CONNECTION_REFUSED: u16 = 102;
pub const IO_CONNECTION_RESET: u16 = 103;
pub const IO_CONNECTION_ABORTED: u16 = 104;
pub const IO_NOT_CONNECTED: u16 = 105;
pub const IO_ADDRESS_IN_USE: u16 = 106;
pub const IO_ADDRESS_NOT_AVAILABLE: u16 = 107;
pub const IO_BROKEN_PIPE: u16 = 108;
pub const IO_ALREADY_EXISTS: u16 = 109;
pub const IO_WOULD_BLOCK: u16 = 110;
pub const IO_INVALID_INPUT: u16 = 111;
pub const IO_INVALID_DATA: u16 = 112;
pub const IO_TIMED_OUT: u16 = 113;
pub const IO_WRITE_ZERO: u16 = 114;
pub const IO_INTERRUPTED: u16 = 115;
pub const IO_UNEXPECTED_EOF: u16 = 116;
pub const IO_NOT_FOUND: u16 = 117;

/// Returns an error code for a Rust IO error.
pub fn from_io_error(error: io::Error) -> u16 {
    match error.kind() {
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
        _ => IO_GENERIC,
    }
}
