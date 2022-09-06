//! Types and methods for hashing objects.
use ahash::AHasher;
use std::hash::{Hash, Hasher as _};
use std::i64;

#[derive(Clone)]
pub(crate) struct Hasher {
    hasher: AHasher,
}

impl Hasher {
    pub(crate) fn new(hasher: AHasher) -> Self {
        Hasher { hasher }
    }

    pub(crate) fn write_int(&mut self, value: i64) {
        value.hash(&mut self.hasher);
    }

    pub(crate) fn finish(&mut self) -> i64 {
        self.hasher.finish() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::RandomState;
    use std::hash::BuildHasher as _;
    use std::mem::size_of;

    #[test]
    fn test_write_int() {
        let state = RandomState::new();
        let mut hasher1 = Hasher::new(state.build_hasher());
        let mut hasher2 = Hasher::new(state.build_hasher());

        hasher1.write_int(10);
        hasher2.write_int(10);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    #[test]
    fn test_mem_size() {
        assert!(size_of::<Hasher>() <= 48);
    }
}
