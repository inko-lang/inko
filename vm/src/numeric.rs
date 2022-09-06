//! Traits and functions for various numerical operations.

/// The Modulo trait is used for getting the modulo (instead of remainder) of a
/// number.
pub(crate) trait Modulo<T = Self> {
    fn checked_modulo(self, rhs: T) -> Option<T>;
}

impl Modulo for i64 {
    fn checked_modulo(self, rhs: i64) -> Option<i64> {
        self.checked_rem(rhs)
            .and_then(|res| res.checked_add(rhs))
            .and_then(|res| res.checked_rem(rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modulo_i64() {
        assert_eq!((-5_i64).checked_modulo(86_400_i64), Some(86395_i64));
        assert_eq!(i64::MIN.checked_modulo(-1_i64), None);
    }
}
