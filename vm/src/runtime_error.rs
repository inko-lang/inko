//! Errors that can be produced at VM runtime.
use crate::error_messages;
use std::convert::From;
use std::io;
use std::net::AddrParseError;

/// An error that can be raised in the VM at runtime.]
#[derive(Debug)]
pub enum RuntimeError {
    /// An error that should be turned into an exception, allowing code to
    /// handle it.
    Exception(String),

    /// A fatal error that should result in the VM terminating.
    Panic(String),

    /// A non-blocking operation would block, and should be retried at a later
    /// point in time.
    WouldBlock,

    /// A non-blocking operation is still in progress and should not be retried.
    /// Instead, the process should be suspended until the operation is done,
    /// after which it should start off at the _next_ instruction.
    InProgress,
}

impl RuntimeError {
    pub fn should_poll(&self) -> bool {
        match self {
            RuntimeError::WouldBlock => true,
            RuntimeError::InProgress => true,
            _ => false,
        }
    }
}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::WouldBlock {
            RuntimeError::WouldBlock
        } else {
            RuntimeError::Exception(error_messages::from_io_error(&error))
        }
    }
}

impl From<String> for RuntimeError {
    fn from(result: String) -> Self {
        RuntimeError::Panic(result)
    }
}

impl From<AddrParseError> for RuntimeError {
    fn from(result: AddrParseError) -> Self {
        RuntimeError::Exception(result.to_string())
    }
}
