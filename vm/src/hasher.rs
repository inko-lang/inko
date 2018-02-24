//! Types and methods for hashing objects.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher as HasherTrait;

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

    pub fn finish(&mut self) -> u64 {
        self.hasher.finish()
    }
}
