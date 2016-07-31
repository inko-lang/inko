//! Tagged Pointers
//!
//! The TaggedPointer struct can be used to wrap a `*mut T` with one (or both)
//! of the lower 2 bits set (or unset).

use std::mem::transmute;
use std::ptr;

macro_rules! bit_within_bounds {
    ($bit: expr) => ({
        assert!($bit <= 1, "Only the lower two bits can be set");
    });
}

/// The mask to use for untagging a pointer.
const UNTAG_MASK: isize = !(0x3 as isize);

/// Structure wrapping a raw, tagged pointer.
#[derive(Debug)]
pub struct TaggedPointer<T> {
    pub raw: *mut T,
}

impl<T> TaggedPointer<T> {
    /// Returns a new TaggedPointer without setting any bits.
    pub fn new(raw: *mut T) -> TaggedPointer<T> {
        TaggedPointer { raw: raw }
    }

    /// Returns a new TaggedPointer with the given bit set.
    pub fn with_bit(raw: *mut T, bit: usize) -> TaggedPointer<T> {
        let mut pointer = Self::new(raw);

        pointer.set_bit(bit);

        pointer
    }

    /// Returns a new TaggedPointer with the given bit mask applied.
    pub fn with_mask(raw: *mut T, mask: usize) -> TaggedPointer<T> {
        let mut pointer = Self::new(raw);

        pointer.set_mask(mask);

        pointer
    }

    /// Returns a null pointer.
    pub fn null() -> TaggedPointer<T> {
        TaggedPointer { raw: ptr::null::<T>() as *mut T }
    }

    /// Returns the wrapped pointer without any tags.
    pub fn untagged(&self) -> *mut T {
        unsafe { transmute(self.raw as isize & UNTAG_MASK) }
    }

    /// Returns true if the given bit is set.
    pub fn bit_is_set(&self, bit: usize) -> bool {
        bit_within_bounds!(bit);

        let shifted = 1 << bit;

        (self.raw as usize & shifted) == shifted
    }

    /// Returns true if the given bit mask is set.
    pub fn mask_is_set(&self, mask: usize) -> bool {
        (self.raw as usize & mask) == mask
    }

    /// Sets the given bit.
    pub fn set_bit(&mut self, bit: usize) {
        bit_within_bounds!(bit);

        self.raw = unsafe { transmute(self.raw as usize | 1 << bit) };
    }

    /// Applies the given bit mask.
    pub fn set_mask(&mut self, mask: usize) {
        self.raw = unsafe { transmute(self.raw as usize | mask) };
    }

    /// Returns true if the current pointer is a null pointer.
    pub fn is_null(&self) -> bool {
        self.untagged().is_null()
    }

    /// Returns an immutable to the pointer's value.
    pub fn as_ref(&self) -> Option<&T> {
        unsafe { self.untagged().as_ref() }
    }

    /// Returns a mutable reference to the pointer's value.
    pub fn as_mut(&self) -> Option<&mut T> {
        unsafe { self.untagged().as_mut() }
    }
}

impl<T> PartialEq for TaggedPointer<T> {
    fn eq(&self, other: &TaggedPointer<T>) -> bool {
        self.raw == other.raw
    }
}

// These traits are implemented manually as "derive" doesn't handle the generic
// "T" argument very well.
impl<T> Clone for TaggedPointer<T> {
    fn clone(&self) -> TaggedPointer<T> {
        TaggedPointer::new(self.raw)
    }
}

impl<T> Copy for TaggedPointer<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn test_with_bit_panics_when_out_of_bounds() {
        let mut name = "Alice".to_string();

        TaggedPointer::with_bit(&mut name as *mut String, 2);
    }

    #[test]
    fn test_untagged() {
        let mut name = "Alice".to_string();
        let ptr = TaggedPointer::with_bit(&mut name as *mut String, 0);

        assert_eq!(unsafe { &*ptr.untagged() }, &name);
    }

    #[test]
    fn test_untagged_with_mask() {
        let mut name = "Alice".to_string();
        let ptr = TaggedPointer::with_mask(&mut name as *mut String, 0x2);

        assert_eq!(unsafe { &*ptr.untagged() }, &name);
    }

    #[test]
    fn test_bit_is_set() {
        let mut name = "Alice".to_string();
        let str_ptr = &mut name as *mut String;

        assert_eq!(TaggedPointer::new(str_ptr).bit_is_set(0), false);
        assert_eq!(TaggedPointer::new(str_ptr).bit_is_set(1), false);

        assert!(TaggedPointer::with_bit(str_ptr, 0).bit_is_set(0));
        assert!(TaggedPointer::with_bit(str_ptr, 1).bit_is_set(1));
    }

    #[test]
    fn test_bit_is_set_with_mask() {
        let mut name = "Alice".to_string();
        let str_ptr = &mut name as *mut String;
        let ptr = TaggedPointer::with_mask(str_ptr, 0x1);

        assert!(ptr.bit_is_set(0));
    }

    #[test]
    fn test_mask_is_set() {
        let mut name = "Alice".to_string();
        let str_ptr = &mut name as *mut String;

        assert_eq!(TaggedPointer::new(str_ptr).mask_is_set(0x1), false);
        assert_eq!(TaggedPointer::new(str_ptr).mask_is_set(0x2), false);

        assert!(TaggedPointer::with_mask(str_ptr, 0x1).mask_is_set(0x1));
        assert!(TaggedPointer::with_mask(str_ptr, 0x2).mask_is_set(0x2));
    }

    #[test]
    #[should_panic]
    fn test_bit_is_set_panics_when_out_of_bounds() {
        let mut name = "Alice".to_string();
        let ptr = TaggedPointer::with_bit(&mut name as *mut String, 0);

        ptr.bit_is_set(2);
    }

    #[test]
    fn test_set_bit() {
        let mut name = "Alice".to_string();
        let str_ptr = &mut name as *mut String;
        let mut ptr = TaggedPointer::new(str_ptr);

        assert_eq!(ptr.bit_is_set(0), false);

        ptr.set_bit(0);

        assert!(ptr.bit_is_set(0));
    }

    #[test]
    fn test_set_mask() {
        let mut name = "Alice".to_string();
        let str_ptr = &mut name as *mut String;
        let mut ptr = TaggedPointer::new(str_ptr);

        assert_eq!(ptr.mask_is_set(0x1), false);

        ptr.set_mask(0x1);

        assert!(ptr.mask_is_set(0x1));
    }

    #[test]
    fn test_eq() {
        let mut name = "Alice".to_string();
        let ptr1 = TaggedPointer::with_bit(&mut name as *mut String, 0);
        let ptr2 = ptr1.clone();

        assert_eq!(ptr1, ptr2);
    }

    #[test]
    fn test_is_null() {
        let mut name = "Alice".to_string();
        let ptr1: TaggedPointer<()> = TaggedPointer::null();
        let ptr2 = TaggedPointer::new(&mut name as *mut String);

        assert_eq!(ptr1.is_null(), true);
        assert_eq!(ptr2.is_null(), false);
    }

    #[test]
    fn test_as_ref() {
        let mut name = "Alice".to_string();
        let ptr = TaggedPointer::new(&mut name as *mut String);

        assert_eq!(ptr.as_ref().unwrap(), &name);
    }

    #[test]
    fn test_as_ref_null() {
        let ptr: TaggedPointer<()> = TaggedPointer::null();

        assert!(ptr.as_ref().is_none());
    }

    #[test]
    fn test_as_mut_null() {
        let ptr: TaggedPointer<()> = TaggedPointer::null();

        assert!(ptr.as_mut().is_none());
    }

    #[test]
    fn test_as_mut() {
        let mut name = "Alice".to_string();
        let ptr = TaggedPointer::new(&mut name as *mut String);

        assert_eq!(ptr.as_mut().unwrap(), &mut name);
    }
}
