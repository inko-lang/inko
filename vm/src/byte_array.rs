use object_pointer::ObjectPointer;
use std::u8;

const MIN_BYTE: i64 = u8::MIN as i64;
const MAX_BYTE: i64 = u8::MAX as i64;

/// Converts a tagged integer to a u8, if possible.
pub fn integer_to_byte(pointer: ObjectPointer) -> Result<u8, String> {
    let value = pointer.integer_value()?;

    if value >= MIN_BYTE && value <= MAX_BYTE {
        Ok(value as u8)
    } else {
        Err(format!(
            "The value {} is not within the range 0..256",
            value
        ))
    }
}
