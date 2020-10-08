//! Types and methods for hashing objects.
use ahash::AHasher;
use num_bigint::BigInt;
use std::hash::{Hash, Hasher as HasherTrait};
use std::i64;
use std::u64;

/// The value to subtract to convert an u64 to an i64.
const U64_I64_DIFF: u64 = u64::MAX - i64::MAX as u64;

#[derive(Clone)]
pub struct Hasher {
    hasher: AHasher,
}

impl Hasher {
    pub fn new(key0: u64, key1: u64) -> Self {
        Hasher {
            hasher: AHasher::new_with_keys(key0 as u128, key1 as u128),
        }
    }

    pub fn write_integer(&mut self, value: i64) {
        value.hash(&mut self.hasher);
    }

    pub fn write_unsigned_integer(&mut self, value: usize) {
        value.hash(&mut self.hasher);
    }

    pub fn write_float(&mut self, value: f64) {
        let bits = self.convert_hash(value.to_bits());

        self.write_integer(bits);
    }

    pub fn write_bigint(&mut self, value: &BigInt) {
        value.hash(&mut self.hasher);
    }

    pub fn write_string(&mut self, value: &str) {
        value.hash(&mut self.hasher);
    }

    pub fn to_hash(&self) -> i64 {
        let hash = self.hasher.finish();

        self.convert_hash(hash)
    }

    fn convert_hash(&self, raw_hash: u64) -> i64 {
        // Hashers produce a u64. This value is usually too large to store as an
        // i64 (even when heap allocating), requiring the use of a bigint. To
        // work around that we subtract the difference between the maximum u64
        // and i64 values, ensuring our final hash value fits in a i64.
        if raw_hash > i64::MAX as u64 {
            (raw_hash - U64_I64_DIFF) as i64
        } else {
            raw_hash as i64 - (U64_I64_DIFF - 1) as i64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::i64;
    use std::mem::size_of;
    use std::u64;

    #[test]
    fn test_write_float() {
        let mut hasher1 = Hasher::new(1, 2);
        let mut hasher2 = Hasher::new(1, 2);

        hasher1.write_float(10.5);
        hasher2.write_float(10.5);

        assert_eq!(hasher1.to_hash(), hasher2.to_hash());
    }

    #[test]
    fn test_write_string() {
        let mut hasher1 = Hasher::new(1, 2);
        let mut hasher2 = Hasher::new(1, 2);
        let string = "hello".to_string();

        hasher1.write_string(&string);
        hasher2.write_string(&string);

        assert_eq!(hasher1.to_hash(), hasher2.to_hash());
    }

    #[test]
    fn test_convert_hash() {
        let hasher = Hasher::new(1, 2);

        assert_eq!(hasher.convert_hash(u64::MAX), 9223372036854775807_i64);
        assert_eq!(hasher.convert_hash(i64::MAX as u64), 0);
        assert_eq!(hasher.convert_hash(0_u64), -9223372036854775807);
        assert_eq!(hasher.convert_hash(1_u64), -9223372036854775806);
        assert_eq!(hasher.convert_hash(2_u64), -9223372036854775805);
    }

    #[test]
    fn test_mem_size() {
        assert!(size_of::<Hasher>() <= 48);
    }
}
