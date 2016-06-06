#![macro_use]

/// Returns a vector index for an i64
macro_rules! int_to_vector_index {
    ($vec: expr, $index: expr) => ({
        if $index >= 0 as i64 {
            $index as usize
        }
        else {
            ($vec.len() as i64 - $index) as usize
        }
    });
}
