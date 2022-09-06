//! Errors for subprocesses.
use getopts::Fail;
use std::io;

/// An error produced by a subprocess.
pub struct Error {
    /// The exit code to use.
    pub status: i32,

    /// An error message to print to STDERR.
    pub message: Option<String>,
}

impl Error {
    pub fn generic(message: String) -> Self {
        Error { status: 1, message: Some(message) }
    }
}

impl From<Fail> for Error {
    fn from(fail: Fail) -> Self {
        Self::generic(fail.to_string())
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::generic(error.to_string())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::generic(message)
    }
}
