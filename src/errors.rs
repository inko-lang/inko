//! Module containing all runtime erorrs and error related functions.
//!
//! These errors are used when a developer is meant to somehow handle them (e.g.
//! when trying to open a non existing file). Critical errors (e.g.  missing
//! instruction arguments) do not rely on these errors.

use std::io;
use std::io::ErrorKind;

pub static IO_OTHER              : &'static str = "io_other";
pub static IO_INVALID_OPEN_MODE  : &'static str = "io_invalid_open_mode";
pub static IO_NOT_FOUND          : &'static str = "io_not_found";
pub static IO_PERMISSION_DENIED  : &'static str = "io_permission_denied";
pub static IO_CONNECTION_REFUSED : &'static str = "io_connection_refused";
pub static IO_CONNECTION_RESET   : &'static str = "io_connection_reset";
pub static IO_CONNECTION_ABORTED : &'static str = "io_connection_aborted";
pub static IO_NOT_CONNECTED      : &'static str = "io_not_connected";
pub static IO_ADDR_IN_USE        : &'static str = "io_address_in_use";
pub static IO_ADDR_NOT_AVAILABLE : &'static str = "io_address_not_available";
pub static IO_BROKEN_PIPE        : &'static str = "io_broken_pipe";
pub static IO_ALREADY_EXISTS     : &'static str = "io_already_exists";
pub static IO_WOULD_BLOCK        : &'static str = "io_would_block";
pub static IO_INVALID_INPUT      : &'static str = "io_invalid_input";
pub static IO_INVALID_DATA       : &'static str = "io_invalid_data";
pub static IO_TIMED_OUT          : &'static str = "io_timed_out";
pub static IO_WRITE_ZERO         : &'static str = "io_write_zero";
pub static IO_INTERRUPTED        : &'static str = "io_interrupted";

pub static STRING_INVALID_UTF8 : &'static str = "string_invalid_utf8";

/// Returns a VM error name for a Rust IO error.
pub fn from_io_error(error: io::Error) -> &'static str {
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
