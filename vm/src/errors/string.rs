#[repr(u16)]
pub enum ErrorKind {
    InvalidUtf8,
}

pub fn invalid_utf8() -> u16 {
    ErrorKind::InvalidUtf8 as u16
}
