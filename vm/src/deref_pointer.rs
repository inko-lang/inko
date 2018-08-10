//! Pointers that automatically dereference to their underlying types.
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

pub struct DerefPointer<T> {
    /// The underlying raw pointer.
    pub pointer: *mut T,
}

unsafe impl<T> Sync for DerefPointer<T> {}
unsafe impl<T> Send for DerefPointer<T> {}

impl<T> DerefPointer<T> {
    pub fn new(value: &T) -> Self {
        DerefPointer {
            pointer: value as *const T as *mut T,
        }
    }

    pub fn from_pointer(value: *mut T) -> Self {
        DerefPointer { pointer: value }
    }

    pub fn null() -> Self {
        DerefPointer {
            pointer: ptr::null_mut(),
        }
    }

    pub fn is_null(self) -> bool {
        self.pointer.is_null()
    }

    /// Atomically swaps the internal pointer with another one.
    ///
    /// This boolean returns true if the pointer was swapped, false otherwise.
    pub fn compare_and_swap(&mut self, current: *mut T, other: *mut T) -> bool {
        self.as_atomic()
            .compare_and_swap(current, other, Ordering::Release)
            == current
    }

    /// Atomically replaces the current pointer with the given one.
    #[cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]
    pub fn atomic_store(&self, other: *mut T) {
        self.as_atomic().store(other, Ordering::Release);
    }

    /// Atomically loads the pointer.
    #[cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]
    pub fn atomic_load(&self) -> Self {
        DerefPointer {
            pointer: self.as_atomic().load(Ordering::Acquire),
        }
    }

    fn as_atomic(&self) -> &AtomicPtr<T> {
        unsafe { &*(self as *const DerefPointer<T> as *const AtomicPtr<T>) }
    }
}

impl<T> Deref for DerefPointer<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.pointer }
    }
}

impl<T> DerefMut for DerefPointer<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.pointer as *mut T) }
    }
}

impl<T> Clone for DerefPointer<T> {
    fn clone(&self) -> DerefPointer<T> {
        DerefPointer {
            pointer: self.pointer,
        }
    }
}

impl<T> Copy for DerefPointer<T> {}

impl<T> PartialEq for DerefPointer<T> {
    fn eq(&self, other: &DerefPointer<T>) -> bool {
        self.pointer == other.pointer
    }
}

impl<T> Eq for DerefPointer<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null() {
        let ptr: DerefPointer<()> = DerefPointer::null();

        assert!(ptr.is_null());
    }

    #[test]
    fn test_deref() {
        let value = "hello";
        let ptr = DerefPointer::new(&value);

        assert_eq!(ptr.to_uppercase(), "HELLO");
    }

    #[test]
    fn test_deref_mut() {
        let value = "hello".to_string();
        let mut ptr = DerefPointer::new(&value);

        ptr.push_str(" world");

        assert_eq!(value, "hello world".to_string());
    }

    #[test]
    fn test_eq() {
        let value = "hello".to_string();
        let ptr1 = DerefPointer::new(&value);
        let ptr2 = DerefPointer::new(&value);

        assert!(ptr1 == ptr2);
    }

    #[test]
    fn test_compare_and_swap() {
        let mut alice = "Alice".to_string();
        let mut bob = "Bob".to_string();

        let mut pointer = DerefPointer::new(&mut alice);
        let current = pointer.pointer;
        let target = &mut bob as *mut String;

        pointer.compare_and_swap(current, target);

        assert!(pointer.pointer == target);
    }

    #[test]
    fn test_atomic_store() {
        let mut alice = "Alice".to_string();
        let mut bob = "Bob".to_string();

        let pointer = DerefPointer::new(&mut alice);
        let target = &mut bob as *mut String;

        pointer.atomic_store(target);

        assert!(pointer.pointer == target);
    }

    #[test]
    fn test_atomic_load() {
        let mut alice = "Alice".to_string();

        let pointer = DerefPointer::new(&mut alice);

        assert!(pointer.atomic_load() == pointer);
    }
}
