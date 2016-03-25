//! Module containing all runtime erorrs and error related functions.
//!
//! These errors are used when a developer is meant to somehow handle them (e.g.
//! when trying to open a non existing file). Critical errors (e.g.  missing
//! instruction arguments) do not rely on these errors.

use std::io;
use std::io::ErrorKind;

pub static IO_OTHER              : u16 = 1;
pub static IO_INVALID_OPEN_MODE  : u16 = 2;
pub static IO_NOT_FOUND          : u16 = 3;
pub static IO_PERMISSION_DENIED  : u16 = 4;
pub static IO_CONNECTION_REFUSED : u16 = 5;
pub static IO_CONNECTION_RESET   : u16 = 6;
pub static IO_CONNECTION_ABORTED : u16 = 7;
pub static IO_NOT_CONNECTED      : u16 = 8;
pub static IO_ADDR_IN_USE        : u16 = 9;
pub static IO_ADDR_NOT_AVAILABLE : u16 = 10;
pub static IO_BROKEN_PIPE        : u16 = 11;
pub static IO_ALREADY_EXISTS     : u16 = 12;
pub static IO_WOULD_BLOCK        : u16 = 13;
pub static IO_INVALID_INPUT      : u16 = 14;
pub static IO_INVALID_DATA       : u16 = 15;
pub static IO_TIMED_OUT          : u16 = 16;
pub static IO_WRITE_ZERO         : u16 = 17;
pub static IO_INTERRUPTED        : u16 = 18;

pub static STRING_INVALID_UTF8 : u16 = 1;

/// Returns a VM error name for a Rust IO error.
pub fn from_io_error(error: io::Error) -> u16 {
    match error.kind() {
        ErrorKind::NotFound          => IO_NOT_FOUND,
        ErrorKind::PermissionDenied  => IO_PERMISSION_DENIED,
        ErrorKind::ConnectionRefused => IO_CONNECTION_REFUSED,
        ErrorKind::ConnectionReset   => IO_CONNECTION_RESET,
        ErrorKind::ConnectionAborted => IO_CONNECTION_ABORTED,
        ErrorKind::NotConnected      => IO_NOT_CONNECTED,
        ErrorKind::AddrInUse         => IO_ADDR_IN_USE,
        ErrorKind::AddrNotAvailable  => IO_ADDR_NOT_AVAILABLE,
        ErrorKind::BrokenPipe        => IO_BROKEN_PIPE,
        ErrorKind::AlreadyExists     => IO_ALREADY_EXISTS,
        ErrorKind::WouldBlock        => IO_WOULD_BLOCK,
        ErrorKind::InvalidInput      => IO_INVALID_INPUT,
        ErrorKind::InvalidData       => IO_INVALID_DATA,
        ErrorKind::TimedOut          => IO_TIMED_OUT,
        ErrorKind::WriteZero         => IO_WRITE_ZERO,
        ErrorKind::Interrupted       => IO_INTERRUPTED,
        _                            => IO_OTHER
    }
}
