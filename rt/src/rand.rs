use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// A simple random number generator based on the fastrand crate.
///
/// This RNG is _not_ cryptographically secure _at all_. Instead, it's purpose
/// is to quickly generate a reasonably random number, to be used for e.g.
/// stealing work from a randomly chosen thread.
pub(crate) struct Rand {
    state: u64,
}

impl Rand {
    pub(crate) fn new() -> Self {
        Self { state: RandomState::new().build_hasher().finish() }
    }

    pub(crate) fn int(&mut self) -> u64 {
        let state = self.state.wrapping_add(0x2d35_8dcc_aa6c_78a5);

        self.state = state;

        let v = u128::from(state) * u128::from(state ^ 0x8bb8_4b93_962e_acc9);

        (v as u64) ^ (v >> 64) as u64
    }

    pub(crate) fn int_between(&mut self, min: u64, max: u64) -> u64 {
        let mut mask = u64::MAX;
        let range = max.wrapping_sub(min).wrapping_sub(1);

        mask >>= (range | 1).leading_zeros();

        loop {
            match self.int() & mask {
                v if v <= range => return min.wrapping_add(v),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rand_int() {
        assert_eq!(Rand { state: 42 }.int(), 14587678697106979209);
    }

    #[test]
    fn test_rand_int_between() {
        assert_eq!(Rand { state: 0 }.int_between(0, 10), 6);
        assert_eq!(Rand { state: 0 }.int_between(5, 10), 6);
        assert_eq!(Rand { state: 0 }.int_between(6, 10), 8);
        assert_eq!(Rand { state: 0 }.int_between(0, 1), 0);
        assert_eq!(Rand { state: 0 }.int_between(1, 2), 1);
        assert_eq!(Rand { state: 1 }.int_between(0, 10), 1);
        assert_eq!(Rand { state: 3 }.int_between(0, 10), 2);
    }
}
