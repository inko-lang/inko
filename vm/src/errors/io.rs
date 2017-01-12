use std::io;

/// The various supported IO errors.
///
/// The order of this enum's variants should not be changed as this will also
/// result in different error codes (breaking compatibility).
#[repr(u16)]
pub enum ErrorKind {
    Other,
    NotFound,
    PermissionDenied,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    NotConnected,
    AddressInUse,
    AddressNotAvailable,
    BrokenPipe,
    AlreadyExists,
    WouldBlock,
    InvalidInput,
    InvalidData,
    TimedOut,
    WriteZero,
    Interrupted,
    UnexpectedEof,
}

/// Returns an error code for a Rust IO error.
pub fn from_io_error(error: io::Error) -> u16 {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => ErrorKind::NotFound,
        io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
        io::ErrorKind::ConnectionRefused => ErrorKind::ConnectionRefused,
        io::ErrorKind::ConnectionReset => ErrorKind::ConnectionReset,
        io::ErrorKind::ConnectionAborted => ErrorKind::ConnectionAborted,
        io::ErrorKind::NotConnected => ErrorKind::NotConnected,
        io::ErrorKind::AddrInUse => ErrorKind::AddressInUse,
        io::ErrorKind::AddrNotAvailable => ErrorKind::AddressNotAvailable,
        io::ErrorKind::BrokenPipe => ErrorKind::BrokenPipe,
        io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
        io::ErrorKind::WouldBlock => ErrorKind::WouldBlock,
        io::ErrorKind::InvalidInput => ErrorKind::InvalidInput,
        io::ErrorKind::InvalidData => ErrorKind::InvalidData,
        io::ErrorKind::TimedOut => ErrorKind::TimedOut,
        io::ErrorKind::WriteZero => ErrorKind::WriteZero,
        io::ErrorKind::Interrupted => ErrorKind::Interrupted,
        io::ErrorKind::UnexpectedEof => ErrorKind::UnexpectedEof,
        _ => ErrorKind::Other,
    };

    kind as u16
}
