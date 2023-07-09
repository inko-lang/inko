use crate::mem::tagged_int;
use std::io;
use std::ptr::null_mut;

const OK: i64 = 0;
const NONE: i64 = 1;
const ERROR: i64 = 2;

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
///
/// We're using a struct here instead of an enum as this gives us more precise
/// control over the layout, and lets us test the exact field offsets.
#[repr(C)]
#[derive(Eq, PartialEq, Debug)]
pub struct Result {
    pub tag: *mut u8,
    pub value: *mut u8,
}

impl Result {
    pub(crate) fn ok(value: *mut u8) -> Result {
        Result { tag: tagged_int(OK) as _, value }
    }

    pub(crate) fn error(value: *mut u8) -> Result {
        Result { tag: tagged_int(ERROR) as _, value }
    }

    pub(crate) fn none() -> Result {
        Result { tag: tagged_int(NONE) as _, value: null_mut() }
    }

    pub(crate) fn ok_boxed<T>(value: T) -> Result {
        Result::ok(Box::into_raw(Box::new(value)) as _)
    }

    pub(crate) fn io_error(error: io::Error) -> Result {
        Self::error(tagged_int(error.raw_os_error().unwrap_or(0) as _) as _)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;
    use std::ptr::addr_of;

    #[test]
    fn test_memory_layout() {
        assert_eq!(size_of::<Result>(), 16);
    }

    #[test]
    fn test_field_offsets() {
        let res = Result::ok(0x4 as _);
        let base = addr_of!(res) as usize;

        assert_eq!(addr_of!(res.tag) as usize - base, 0);
        assert_eq!(addr_of!(res.value) as usize - base, 8);
    }
}
