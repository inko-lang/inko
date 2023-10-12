use fnv::{FnvHashMap, FnvHashSet};

/// A type for generating method hash codes.
///
/// These hash codes are used as part of dynamic dispatch. Each method name is
/// given a globally unique hash code. We don't need to consider the entire
/// method's signature as Inko doesn't allow overloading of methods.
///
/// The algorithm used by this hasher is FNV-1a, as it's one of the fastest
/// not-so-terrible hash function for small inputs.
pub(crate) struct MethodHasher {
    hashes: FnvHashMap<String, u64>,
    used: FnvHashSet<u64>,
}

impl MethodHasher {
    pub(crate) fn new() -> MethodHasher {
        // We can't predict how many unique method names there are without
        // counting them, which would involve hashing, which in turn likely
        // wouldn't make this hasher any faster.
        //
        // Instead we conservatively assume every program needs at least this
        // many slots, reducing the amount of rehashing necessary without
        // reserving way too much memory.
        let size = 512;

        MethodHasher {
            hashes: FnvHashMap::with_capacity_and_hasher(
                size,
                Default::default(),
            ),
            used: FnvHashSet::with_capacity_and_hasher(
                size,
                Default::default(),
            ),
        }
    }

    pub(crate) fn hash(&mut self, name: String) -> u64 {
        if let Some(&hash) = self.hashes.get(&name) {
            return hash;
        }

        let mut base = 0xcbf29ce484222325;

        for &byte in name.as_bytes() {
            base = self.round(base, byte as u64);
        }

        // Bytes are in the range from 0..255. By starting the extra value at
        // 256 we're (hopefully) less likely to produce collisions with method
        // names that are one byte longer than our current method name.
        let mut extra = 256_u64;
        let mut hash = base;

        // FNV isn't a perfect hash function, so collisions are possible. In
        // this case we just add a number to the base hash until we produce a
        // unique hash.
        while self.used.contains(&hash) {
            hash = self.round(base, extra);
            extra = extra.wrapping_add(1);
        }

        self.hashes.insert(name, hash);
        self.used.insert(hash);
        hash
    }

    fn round(&self, hash: u64, value: u64) -> u64 {
        (hash ^ value).wrapping_mul(0x100_0000_01b3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash() {
        let mut hasher = MethodHasher::new();

        assert_eq!(
            hasher.hash("foo".to_string()),
            hasher.hash("foo".to_string())
        );
        assert_ne!(
            hasher.hash("foo".to_string()),
            hasher.hash("bar".to_string())
        );
    }

    #[test]
    fn test_hash_conflict() {
        let mut hasher = MethodHasher::new();

        let hash = hasher.hash("foo".to_string());

        hasher.hashes.remove("foo");

        assert_ne!(hasher.hash("foo".to_string()), hash);
    }
}
