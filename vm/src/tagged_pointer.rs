//! Tagged Pointers
//!
//! The TaggedPointer struct can be used to wrap a `*mut T` with one (or both)
//! of the lower 2 bits set (or unset).

use std::hash::{Hash, Hasher};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

/// The mask to use for untagging a pointer.
const UNTAG_MASK: usize = (!0x7) as usize;

/// Returns true if the pointer has the given bit set to 1.
pub fn bit_is_set<T>(pointer: *mut T, bit: usize) -> bool {
    let shifted = 1 << bit;

    (pointer as usize & shifted) == shifted
}

/// Returns the pointer with the given bit set.
pub fn with_bit<T>(pointer: *mut T, bit: usize) -> *mut T {
    (pointer as usize | 1 << bit) as _
}

/// Returns the given pointer without any tags set.
pub fn untagged<T>(pointer: *mut T) -> *mut T {
    (pointer as usize & UNTAG_MASK) as _
}

/// Structure wrapping a raw, tagged pointer.
#[derive(Debug)]
pub struct TaggedPointer<T> {
    pub raw: *mut T,
}

impl<T> TaggedPointer<T> {
    /// Returns a new TaggedPointer without setting any bits.
    pub fn new(raw: *mut T) -> TaggedPointer<T> {
        TaggedPointer { raw }
    }

    /// Returns a new TaggedPointer with the given bit set.
    pub fn with_bit(raw: *mut T, bit: usize) -> TaggedPointer<T> {
        let mut pointer = Self::new(raw);

        pointer.set_bit(bit);

        pointer
    }

    /// Returns a null pointer.
    pub fn null() -> TaggedPointer<T> {
        TaggedPointer {
            raw: ptr::null::<T>() as *mut T,
        }
    }

    /// Returns the wrapped pointer without any tags.
    pub fn untagged(self) -> *mut T {
        self::untagged(self.raw)
    }

    /// Returns a new TaggedPointer using the current pointer but without any
    /// tags.
    pub fn without_tags(self) -> Self {
        Self::new(self.untagged())
    }

    /// Returns true if the given bit is set.
    pub fn bit_is_set(self, bit: usize) -> bool {
        self::bit_is_set(self.raw, bit)
    }

    /// Sets the given bit.
    pub fn set_bit(&mut self, bit: usize) {
        self.raw = with_bit(self.raw, bit);
    }

    /// Returns true if the current pointer is a null pointer.
    pub fn is_null(self) -> bool {
        self.untagged().is_null()
    }

    /// Returns an immutable to the pointer's value.
    pub fn as_ref<'a>(self) -> Option<&'a T> {
        unsafe { self.untagged().as_ref() }
    }

    /// Returns a mutable reference to the pointer's value.
    pub fn as_mut<'a>(self) -> Option<&'a mut T> {
        unsafe { self.untagged().as_mut() }
    }

    /// Atomically swaps the internal pointer with another one.
    ///
    /// This boolean returns true if the pointer was swapped, false otherwise.
    #[cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]
    pub fn compare_and_swap(&self, current: *mut T, other: *mut T) -> bool {
        self.as_atomic()
            .compare_and_swap(current, other, Ordering::AcqRel)
            == current
    }

    /// Atomically replaces the current pointer with the given one.
    #[cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]
    pub fn atomic_store(&self, other: *mut T) {
        self.as_atomic().store(other, Ordering::Release);
    }

    /// Atomically loads the pointer.
    #[cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]
    pub fn atomic_load(&self) -> *mut T {
        self.as_atomic().load(Ordering::Acquire)
    }

    /// Checks if a bit is set using an atomic load.
    #[cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]
    pub fn atomic_bit_is_set(&self, bit: usize) -> bool {
        Self::new(self.atomic_load()).bit_is_set(bit)
    }

    fn as_atomic(&self) -> &AtomicPtr<T> {
        unsafe { &*(self as *const TaggedPointer<T> as *const AtomicPtr<T>) }
    }
}

impl<T> PartialEq for TaggedPointer<T> {
    fn eq(&self, other: &TaggedPointer<T>) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for TaggedPointer<T> {}

// These traits are implemented manually as "derive" doesn't handle the generic
// "T" argument very well.
impl<T> Clone for TaggedPointer<T> {
    fn clone(&self) -> TaggedPointer<T> {
        TaggedPointer::new(self.raw)
    }
}

impl<T> Copy for TaggedPointer<T> {}

impl<T> Hash for TaggedPointer<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_untagged() {
        let mut name = "Alice".to_string();
        let ptr = TaggedPointer::with_bit(&mut name as *mut String, 0);

        assert_eq!(unsafe { &*ptr.untagged() }, &name);
    }

    #[test]
    fn test_without_tags() {
        let mut name = "Alice".to_string();
        let ptr = TaggedPointer::with_bit(&mut name as *mut String, 0);

        let without_tags = ptr.without_tags();

        assert_eq!(without_tags.bit_is_set(0), false);
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
    fn test_set_bit() {
        let mut name = "Alice".to_string();
        let str_ptr = &mut name as *mut String;
        let mut ptr = TaggedPointer::new(str_ptr);

        assert_eq!(ptr.bit_is_set(0), false);

        ptr.set_bit(0);

        assert!(ptr.bit_is_set(0));
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

    #[test]
    fn test_hash() {
        let mut set = HashSet::new();
        let mut alice = "Alice".to_string();
        let mut bob = "Bob".to_string();

        let ptr1 = TaggedPointer::new(&mut alice as *mut String);
        let ptr2 = TaggedPointer::new(&mut alice as *mut String);
        let ptr3 = TaggedPointer::new(&mut bob as *mut String);

        set.insert(ptr1);
        set.insert(ptr2);

        assert!(set.contains(&ptr1));
        assert!(set.contains(&ptr2));

        assert_eq!(set.contains(&ptr3), false);
    }

    #[test]
    fn test_compare_and_swap() {
        let mut alice = "Alice".to_string();
        let mut bob = "Bob".to_string();

        let pointer = TaggedPointer::new(&mut alice as *mut String);
        let current = pointer.raw;
        let target = &mut bob as *mut String;

        pointer.compare_and_swap(current, target);

        assert!(pointer.raw == target);
    }

    #[test]
    fn test_atomic_store() {
        let mut alice = "Alice".to_string();
        let mut bob = "Bob".to_string();

        let pointer = TaggedPointer::new(&mut alice as *mut String);
        let target = &mut bob as *mut String;

        pointer.atomic_store(target);

        assert!(pointer.raw == target);
    }

    #[test]
    fn test_atomic_load() {
        let mut alice = "Alice".to_string();

        let pointer = TaggedPointer::new(&mut alice as *mut String);

        assert_eq!(pointer.atomic_load(), &mut alice as *mut String);
    }

    #[test]
    fn test_atomic_bit_is_set() {
        let mut pointer = TaggedPointer::new(0x10 as *mut ());

        assert_eq!(pointer.atomic_bit_is_set(0), false);

        pointer.set_bit(0);

        assert!(pointer.atomic_bit_is_set(0));
    }
}
