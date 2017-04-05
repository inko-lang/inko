//! Pointers that automatically dereference to their underlying types.
use std::ops::Deref;
use std::ops::DerefMut;

pub struct DerefPointer<T> {
    pointer: *const T,
}

unsafe impl<T> Sync for DerefPointer<T> {}
unsafe impl<T> Send for DerefPointer<T> {}

impl<T> DerefPointer<T> {
    pub fn new(value: &T) -> Self {
        DerefPointer { pointer: value as *const T }
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
        DerefPointer { pointer: self.pointer }
    }
}

impl<T> Copy for DerefPointer<T> {}

#[cfg(test)]
mod tests {
    use super::*;

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
}
