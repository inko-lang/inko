//! Functions for slicing operations on Vec and String types.
use crate::numeric::modulo::Modulo;
use crate::object_pointer::ObjectPointer;
use num_bigint::BigInt;
use num_traits::sign::Signed;
use num_traits::ToPrimitive;
use std::i128;
use std::i64;

// Returns the slice index to use for accessing an element, with support for
// negative indexes.
//
// When a negative index is used, the final index is determined by counting
// backwards. For example, for a slice with 3 values an index of -1 will map to
// an index of 2. Some other examples, all for a slice with 3 values:
//
// * -1 maps to 2
// * -2 maps to 1
// * -3 maps to 0
// * -4 maps to 2
pub fn slice_index_to_usize(
    index: ObjectPointer,
    length: usize,
) -> Result<usize, String> {
    if let Ok(val) = index.integer_value() {
        if val >= 0 {
            Ok(val as usize)
        } else if length == 0 {
            Ok(0)
        } else if (length as u64) > (i64::MAX as u64) {
            // If the number of values in a slice is greater than we can fit in
            // a i64 (= the type of the index), we upcast both to the index and
            // the length to a i128, then convert this to our final value.
            //
            // Casting the result back to usize should be safe because, as on a
            // 32 bits platform "length" is never greater than the maximum i64
            // value, and an index can never be greater than an i64 either.
            //
            // This is a bit of a hack, and I'm sure there's a better way of
            // doing this (apart from not supporting negative slice indexes).
            // Unfortunately, at the time of writing this was the best I could
            // come up with.
            Ok((i128::from(val)).modulo(length as i128) as usize)
        } else {
            Ok(val.modulo(length as i64) as usize)
        }
    } else if let Ok(val) = index.bigint_value() {
        let result = if val.is_negative() {
            val.clone().modulo(&BigInt::from(length)).to_usize()
        } else {
            val.to_usize()
        };

        result.ok_or_else(|| {
            format!("{} is too big to convert to an integer of type usize", val)
        })
    } else {
        Err("The index is not an integer".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::immix::block::Block;
    use crate::object::Object;
    use crate::object_value;

    #[test]
    fn test_slice_index_to_usize_with_no_values() {
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(0), 0), Ok(0));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(1), 0), Ok(1));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(-5), 0), Ok(0));
    }

    #[test]
    fn test_slice_index_to_usize_with_values() {
        let mut block = Block::boxed();

        assert_eq!(slice_index_to_usize(ObjectPointer::integer(0), 3), Ok(0));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(1), 3), Ok(1));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(-1), 3), Ok(2));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(-2), 3), Ok(1));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(-3), 3), Ok(0));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(-4), 3), Ok(2));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(-5), 3), Ok(1));
        assert_eq!(slice_index_to_usize(ObjectPointer::integer(-6), 3), Ok(0));

        assert_eq!(
            slice_index_to_usize(
                ObjectPointer::integer(9_663_676_416),
                10_737_418_240
            ),
            Ok(9_663_676_416)
        );

        assert_eq!(
            slice_index_to_usize(
                ObjectPointer::integer(-9_663_676_416),
                10_737_418_240
            ),
            Ok(1_073_741_824)
        );

        assert_eq!(
            slice_index_to_usize(
                Object::new(object_value::integer(-9_223_372_036_854_775_807))
                    .write_to(block.request_pointer().unwrap()),
                18_446_744_073_709_551_615
            ),
            Ok(9_223_372_036_854_775_808)
        );
    }

    #[test]
    fn test_index_with_positive_bigint() {
        let mut block = Block::boxed();
        let ptr = Object::new(object_value::bigint(BigInt::from(4)))
            .write_to(block.request_pointer().unwrap());

        assert_eq!(slice_index_to_usize(ptr, 10), Ok(4));

        block.finalize();
    }

    #[test]
    fn test_index_with_negative_bigint() {
        let mut block = Block::boxed();
        let ptr = Object::new(object_value::bigint(BigInt::from(-4)))
            .write_to(block.request_pointer().unwrap());

        assert_eq!(slice_index_to_usize(ptr, 10), Ok(6));

        block.finalize();
    }

    #[test]
    fn test_index_with_too_big_bigint() {
        let mut block = Block::boxed();
        let ptr = Object::new(object_value::bigint(BigInt::from(i128::MAX)))
            .write_to(block.request_pointer().unwrap());

        assert!(slice_index_to_usize(ptr, 10).is_err());

        block.finalize();
    }
}
