//! Thread-safe reference counting pointers, without weak pointers.
//!
//! ArcWithoutWeak is a pointer similar to Rust's Arc type, except no weak
//! references are supported. This makes ArcWithoutWeak ideal for performance
//! sensitive code where weak references are not needed.

use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The inner value of a pointer.
pub struct Inner<T> {
    value: T,
    references: AtomicUsize,
}

/// A thread-safe reference counted pointer.
pub struct ArcWithoutWeak<T> {
    inner: *mut Inner<T>,
}

unsafe impl<T> Sync for ArcWithoutWeak<T> {}
unsafe impl<T> Send for ArcWithoutWeak<T> {}

impl<T> ArcWithoutWeak<T> {
    pub fn new(value: T) -> Self {
        let inner = Inner {
            value,
            references: AtomicUsize::new(1),
        };

        ArcWithoutWeak {
            inner: Box::into_raw(Box::new(inner)),
        }
    }

    pub fn inner(&self) -> &Inner<T> {
        unsafe { &(*self.inner) }
    }

    pub fn references(&self) -> usize {
        self.inner().references.load(Ordering::SeqCst)
    }
}

impl<T> Deref for ArcWithoutWeak<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &(*self.inner).value }
    }
}

impl<T> DerefMut for ArcWithoutWeak<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut (*self.inner).value }
    }
}

impl<T> Clone for ArcWithoutWeak<T> {
    fn clone(&self) -> ArcWithoutWeak<T> {
        self.inner().references.fetch_add(1, Ordering::Relaxed);

        ArcWithoutWeak { inner: self.inner }
    }
}

impl<T> Drop for ArcWithoutWeak<T> {
    fn drop(&mut self) {
        unsafe {
            if self.inner().references.fetch_sub(1, Ordering::Release) == 1 {
                let boxed = Box::from_raw(self.inner as *mut Inner<T>);

                drop(boxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deref() {
        let pointer = ArcWithoutWeak::new(10);

        assert_eq!(*pointer, 10);
    }

    #[test]
    fn test_clone() {
        let pointer = ArcWithoutWeak::new(10);
        let cloned = pointer.clone();

        assert_eq!(pointer.references(), 2);
        assert_eq!(cloned.references(), 2);
    }

    #[test]
    fn test_drop() {
        let pointer = ArcWithoutWeak::new(10);
        let cloned = pointer.clone();

        drop(cloned);

        assert_eq!(pointer.references(), 1);
    }
}
