//! Modulo operations for numeric types.
//!
//! The Modulo trait can be used for getting the modulo (instead of remainder)
//! of a number.
use num_bigint::BigInt;

pub trait Modulo<RHS = Self> {
    type Output;

    fn modulo(self, rhs: RHS) -> Self::Output;
}

pub trait OverflowingModulo<RHS = Self> {
    type Output;

    fn overflowing_modulo(self, rhs: RHS) -> (Self::Output, bool);
}

impl Modulo for i64 {
    type Output = i64;

    fn modulo(self, rhs: Self) -> Self::Output {
        ((self % rhs) + rhs) % rhs
    }
}

impl OverflowingModulo for i64 {
    type Output = i64;

    fn overflowing_modulo(self, rhs: Self) -> (Self::Output, bool) {
        if self == Self::min_value() && rhs == -1 {
            (0, true)
        } else {
            (self.modulo(rhs), false)
        }
    }
}

impl Modulo for i128 {
    type Output = i128;

    fn modulo(self, rhs: Self) -> Self::Output {
        ((self % rhs) + rhs) % rhs
    }
}

impl<'a> Modulo<&'a BigInt> for BigInt {
    type Output = BigInt;

    fn modulo(self, rhs: &Self) -> Self::Output {
        ((self % rhs) + rhs) % rhs
    }
}

impl Modulo<i32> for BigInt {
    type Output = BigInt;

    fn modulo(self, rhs: i32) -> Self::Output {
        ((self % rhs) + rhs) % rhs
    }
}

impl Modulo for BigInt {
    type Output = BigInt;

    fn modulo(self, rhs: Self) -> Self::Output {
        self.modulo(&rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modulo_i64() {
        assert_eq!((-5_i64).modulo(86_400_i64), 86395_i64);
    }

    #[test]
    fn test_modulo_i128() {
        assert_eq!((-5_i128).modulo(86_400_i128), 86_395_i128);
    }

    #[test]
    fn test_modulo_big_integer_with_i64() {
        let a = BigInt::from(-5);
        let b = 86_400;

        assert_eq!(a.modulo(b), BigInt::from(86395));
    }

    #[test]
    fn test_modulo_big_integer_with_big_integer() {
        let a = BigInt::from(-5);
        let b = BigInt::from(86_400);

        assert_eq!(a.modulo(&b), BigInt::from(86395));
    }
}
