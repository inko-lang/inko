//! Helper functionality for Unix based network pollers.
use nix;
use std::io;

pub fn map_error<T>(error: nix::Result<T>) -> io::Result<T> {
    error.map_err(|err| io::Error::from(err.as_errno().unwrap()))
}
