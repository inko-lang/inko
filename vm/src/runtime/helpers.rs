use std::io::{self, Read};

/// Reads a number of bytes from a buffer into a Vec.
pub(crate) fn read_into<T: Read>(
    stream: &mut T,
    output: &mut Vec<u8>,
    size: i64,
) -> Result<i64, io::Error> {
    let read = if size > 0 {
        stream.take(size as u64).read_to_end(output)?
    } else {
        stream.read_to_end(output)?
    };

    Ok(read as i64)
}
