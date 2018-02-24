//! Types and methods for hashing objects.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher as HasherTrait;
use std::i64;
use std::u64;

const U64_I64_DIFF: u64 = u64::MAX - i64::MAX as u64;

#[derive(Clone)]
pub struct Hasher {
    hasher: DefaultHasher,
}

impl Hasher {
    pub fn new() -> Self {
        Hasher {
            hasher: DefaultHasher::new(),
        }
    }

    pub fn write(&mut self, value: i64) {
        self.hasher.write_i64(value);
    }

    pub fn finish(&mut self) -> i64 {
        // Rust's hasher produces a u64. This value is usually too large to
        // store as an i64 (even when heap allocating), requiring the use of a
        // bigint. To work around that we subtract the difference between the
        // maximum u64 and i64 values, ensuring our final hash value fits in a
        // i64.
        (self.hasher.finish() - U64_I64_DIFF) as i64
    }
}
