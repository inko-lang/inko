//! Floored integer divisions.

use num_bigint::BigInt;
use num_integer;
use num_integer::Integer;

pub trait FlooredDiv<RHS = Self> {
    type Output;

    fn floored_division(self, rhs: RHS) -> Self::Output;
}

pub trait OverflowingFlooredDiv<RHS = Self> {
    type Output;

    fn overflowing_floored_division(self, rhs: RHS) -> (Self::Output, bool);
}

impl FlooredDiv for i64 {
    type Output = i64;

    fn floored_division(self, rhs: Self) -> Self::Output {
        num_integer::div_floor(self, rhs)
    }
}

impl OverflowingFlooredDiv for i64 {
    type Output = i64;

    fn overflowing_floored_division(self, rhs: Self) -> (Self::Output, bool) {
        if self == Self::min_value() && rhs == -1 {
            (self, true)
        } else {
            (self.floored_division(rhs), false)
        }
    }
}

impl FlooredDiv for BigInt {
    type Output = BigInt;

    fn floored_division(self, rhs: Self) -> Self::Output {
        self.div_floor(&rhs)
    }
}

impl FlooredDiv<i32> for BigInt {
    type Output = BigInt;

    fn floored_division(self, rhs: i32) -> Self::Output {
        let rhs = BigInt::from(rhs);
        self.div_floor(&rhs)
    }
}

impl<'a> FlooredDiv<&'a BigInt> for BigInt {
    type Output = BigInt;

    fn floored_division(self, rhs: &Self) -> Self::Output {
        self.div_floor(rhs)
    }
}
