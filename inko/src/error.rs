//! Errors for subprocesses.
use getopts::Fail;
use std::io;

/// An error produced by a subprocess.
pub(crate) struct Error {
    /// The exit code to use.
    pub(crate) status: i32,

    /// An error message to print to STDERR.
    pub(crate) message: Option<String>,
}

impl From<Fail> for Error {
    fn from(fail: Fail) -> Self {
        Self::from(fail.to_string())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Error { status: 1, message: Some(message) }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error { status: 1, message: Some(error.to_string()) }
    }
}
