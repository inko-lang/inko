use crate::mem::tagged_int;
use std::io;
use std::mem::swap;

const INVALID_INPUT: i64 = 11;
const TIMED_OUT: i64 = 13;

/// A result type that is FFI safe and wraps a pointer.
///
/// Various functions in the runtime library need a way to signal an OK versus
/// an error value. Some of these errors are simple IO error codes, while others
/// may be strings or something else. Rust's built-in `Result` type isn't FFI
/// safe and as such we can't use it in our runtime functions.
///
/// This type is essentially Rust's `Result` type, minus any methods as we don't
/// use it as output and not input. The layout is fixed so generated code can
/// use it as if this type were defined in the generated code directly.
///
/// The order of this type is and must stay fixed, as rearranging the order of
/// the variants breaks generated code (unless it too is updated accordingly).
#[repr(C, u8)]
#[derive(Eq, PartialEq, Debug)]
pub enum Result {
    /// The operation succeeded.
    Ok(*mut u8),

    /// The operation failed.
    Error(*mut u8),

    /// No result is produced just yet.
    None,
}

impl Result {
    pub(crate) fn ok_boxed<T>(value: T) -> Result {
        Result::Ok(Box::into_raw(Box::new(value)) as _)
    }

    pub(crate) fn io_error(error: io::Error) -> Result {
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
            io::ErrorKind::TimedOut => TIMED_OUT,
            io::ErrorKind::WriteZero => 14,
            io::ErrorKind::Interrupted => 15,
            io::ErrorKind::UnexpectedEof => 16,
            _ => 0,
        };

        Self::Error(tagged_int(code) as _)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<Result>(), 16);
    }
}
