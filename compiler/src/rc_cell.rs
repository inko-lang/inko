//! Reference counted types with automatic dereferencing while allow mutations
//! of the inner value.

use std::cell::UnsafeCell;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;
use std::rc::Rc;

pub struct RcCell<T> {
    inner: Rc<UnsafeCell<T>>,
}

unsafe impl<T> Sync for RcCell<T> {}
unsafe impl<T> Send for RcCell<T> {}

impl<T> RcCell<T> {
    pub fn new(value: T) -> Self {
        RcCell { inner: Rc::new(UnsafeCell::new(value)) }
    }
}

impl<T> Deref for RcCell<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*(self.inner.get() as *const T) }
    }
}

impl<T> DerefMut for RcCell<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.inner.get() }
    }
}

impl<T: fmt::Debug> fmt::Debug for RcCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T> Clone for RcCell<T> {
    fn clone(&self) -> RcCell<T> {
        RcCell { inner: self.inner.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deref() {
        let ptr = RcCell::new("hello".to_string());

        assert_eq!(ptr.to_uppercase(), "HELLO");
    }

    #[test]
    fn test_deref_mut() {
        let mut ptr = RcCell::new("hello".to_string());

        ptr.push_str(" world");

        assert_eq!(value, "hello world".to_string());
    }
}
