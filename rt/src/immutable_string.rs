//! Immutable strings that can be shared with C code.
use std::fmt;
use std::ops::Deref;
use std::str;

const NULL_BYTE: u8 = 0;

/// An immutable string that can be shared with C code.
///
/// An ImmutableString is similar to Rust's `String` type, except it is
/// immutable and adds a NULL byte to the end. Despite the use of a trailing
/// NULL byte, an `ImmutableString` stores its length separately, allowing you
/// to still store NULL bytes any where in the `ImmutableString`.
#[derive(Eq, PartialEq, Hash, Clone)]
pub(crate) struct ImmutableString {
    bytes: Box<[u8]>,
}

#[cfg_attr(feature = "cargo-clippy", allow(clippy::len_without_is_empty))]
impl ImmutableString {
    /// Creates an `ImmutableString` from a `Vec<u8>`, replacing any invalid
    /// UTF-8 sequences with `U+FFFD REPLACEMENT CHARACTER`.
    pub(crate) fn from_utf8(bytes: Vec<u8>) -> Self {
        let string = match String::from_utf8(bytes) {
            Ok(string) => string,
            Err(err) => String::from_utf8_lossy(&err.into_bytes()).into_owned(),
        };

        Self::from(string)
    }

    /// Returns a string slice pointing to the underlying bytes.
    ///
    /// The returned slice _does not_ include the NULL byte.
    pub(crate) fn as_slice(&self) -> &str {
        unsafe { str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Returns a reference to the underlying bytes.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[0..self.len()]
    }

    /// Returns the number of bytes in this String.
    pub(crate) fn len(&self) -> usize {
        self.bytes.len() - 1
    }

    /// Returns a pointer to the bytes, including the NULL byte.
    pub(crate) fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr() as *const _
    }
}

impl Deref for ImmutableString {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_slice()
    }
}

impl From<Vec<u8>> for ImmutableString {
    /// Creates an `ImmutableString` from a `Vec<u8>`, without checking if the
    /// input is valid UTF-8.
    fn from(mut bytes: Vec<u8>) -> Self {
        bytes.reserve_exact(1);
        bytes.push(NULL_BYTE);

        ImmutableString { bytes: bytes.into_boxed_slice() }
    }
}

impl From<String> for ImmutableString {
    fn from(string: String) -> Self {
        Self::from(string.into_bytes())
    }
}

impl fmt::Display for ImmutableString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.as_slice(), f)
    }
}

impl fmt::Debug for ImmutableString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self.as_slice(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_utf8() {
        let valid = ImmutableString::from_utf8(vec![105, 110, 107, 111]);
        let invalid = ImmutableString::from_utf8(vec![
            72, 101, 108, 108, 111, 32, 240, 144, 128, 87, 111, 114, 108, 100,
        ]);

        assert_eq!(valid.as_slice(), "inko");
        assert_eq!(invalid.as_slice(), "Hello �World");
    }

    #[test]
    fn test_as_slice() {
        let string = ImmutableString::from("hello".to_string());

        assert_eq!(string.as_slice(), "hello");
    }

    #[test]
    fn test_as_bytes() {
        let string = ImmutableString::from("inko".to_string());

        assert_eq!(string.as_bytes(), &[105, 110, 107, 111]);
    }

    #[test]
    fn test_len() {
        let string = ImmutableString::from("inko".to_string());

        assert_eq!(string.len(), 4);
    }

    #[test]
    fn test_from_bytes() {
        let string = ImmutableString::from(vec![10]);

        assert_eq!(string.bytes.as_ref(), &[10, 0]);
    }

    #[test]
    fn test_deref_string_slice() {
        let string = ImmutableString::from("inko".to_string());

        assert_eq!(&*string, "inko");
    }
}
