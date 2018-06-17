use object_pointer::ObjectPointer;
use std::io::{Error as IOError, Read};

pub enum ReadResult<T> {
    /// The value to return in case of a successful operation.
    Ok(T),

    /// A type of error that can be handled during runtime.
    Err(IOError),

    /// A type of error that should result in the VM terminating with a panic.
    Panic(String),
}

/// Reads a number of bytes from a stream into a byte array.
pub fn read_from_stream(
    stream: &mut Read,
    buffer: &mut Vec<u8>,
    amount: ObjectPointer,
) -> ReadResult<usize> {
    let result = if amount.is_integer() {
        let amount_bytes = amount.integer_value().unwrap();

        if amount_bytes < 0 {
            return ReadResult::Panic(format!(
                "{} is not a valid number of bytes to read",
                amount_bytes
            ));
        }

        stream.take(amount_bytes as u64).read_to_end(buffer)
    } else {
        stream.read_to_end(buffer)
    };

    // When reading into a buffer, the Vec type may decide to grow it beyond the
    // necessary size. This can lead to a waste of memory, especially when the
    // buffer only sticks around for a short amount of time. To work around
    // this we manually shrink the buffer once we're done writing.
    buffer.shrink_to_fit();

    match result {
        Ok(amount) => ReadResult::Ok(amount),
        Err(err) => ReadResult::Err(err),
    }
}
