//! Errors that can be produced at VM runtime.
use crate::mem::Pointer;
use std::convert::From;
use std::io;
use std::net::AddrParseError;

const INVALID_INPUT: i64 = 11;

/// An error that can be raised in the VM at runtime.
#[derive(Debug)]
pub(crate) enum RuntimeError {
    /// An error to throw as-is.
    Error(Pointer),

    /// A fatal error that should result in the VM terminating.
    Panic(String),

    /// A non-blocking operation would block, and should be retried at a later
    /// point in time.
    WouldBlock,
}

impl RuntimeError {
    pub(crate) fn should_poll(&self) -> bool {
        matches!(self, RuntimeError::WouldBlock)
    }
}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::WouldBlock {
            RuntimeError::WouldBlock
        } else {
            let code = match error.kind() {
                io::ErrorKind::NotFound => 1,
                io::ErrorKind::PermissionDenied => 2,
                io::ErrorKind::ConnectionRefused => 3,
                io::ErrorKind::ConnectionReset => 4,
                io::ErrorKind::ConnectionAborted => 5,
                io::ErrorKind::NotConnected => 6,
                io::ErrorKind::AddrInUse => 7,
                io::ErrorKind::AddrNotAvailable => 8,
                io::ErrorKind::BrokenPipe => 9,
                io::ErrorKind::AlreadyExists => 10,
                io::ErrorKind::InvalidInput => INVALID_INPUT,
                io::ErrorKind::InvalidData => 12,
                io::ErrorKind::TimedOut => 13,
                io::ErrorKind::WriteZero => 14,
                io::ErrorKind::Interrupted => 15,
                io::ErrorKind::UnexpectedEof => 16,
                _ => 0,
            };

            RuntimeError::Error(Pointer::int(code))
        }
    }
}

impl From<String> for RuntimeError {
    fn from(result: String) -> Self {
        RuntimeError::Panic(result)
    }
}

impl From<&str> for RuntimeError {
    fn from(result: &str) -> Self {
        RuntimeError::Panic(result.to_string())
    }
}

impl From<AddrParseError> for RuntimeError {
    fn from(_: AddrParseError) -> Self {
        RuntimeError::Error(Pointer::int(INVALID_INPUT))
    }
}
