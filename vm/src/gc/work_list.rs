//! Lists of pointers to mark.
//!
//! A WorkList can be used to store and retrieve the pointers to mark during the
//! tracing phase of a garbage collection cycle.
//!
//! # Prefetching
//!
//! The WorkList structure uses prefetching (when supported) to reduce the
//! amount of cache misses. The code used for this is based on the technique
//! described in "Effective Prefetch for Mark-Sweep Garbage Collection" by
//! Garner et al (2007). A PDF version of this paper can be found at
//! <http://users.cecs.anu.edu.au/~steveb/downloads/pdf/pf-ismm-2007.pdf>.
//!
//! If the buffer is empty we prefetch up to 8 pointers, this improves
//! performance drastically compared to just prefetching one pointer at a time.
//! In the best case scenario using this technique can improve tracing
//! performance by 20-30%.

#![cfg(feature = "prefetch")]
use std::intrinsics;

use object_pointer::ObjectPointerPointer;
use std::collections::VecDeque;

/// The number of pointers to prefetch when the buffer is empty. This size is
/// based on the number of pointers that fit in a typical single cache line (=
/// 64 bytes on most common processors).
pub const PREFETCH_BUFFER_SIZE: usize = 8;

/// The amount of values to reserve space for in the mark stack. We anticipate
/// many pointers to be stored so preallocating this space reduces the number
/// of reallocations necessary.
pub const STACK_RESERVE_SIZE: usize = 128;

pub struct WorkList {
    /// All the pointers that need to be marked.
    pub stack: VecDeque<ObjectPointerPointer>,

    /// Pointers that have already been prefetched and should be marked before
    /// those in the stack.
    pub prefetch_buffer: VecDeque<ObjectPointerPointer>,
}

impl WorkList {
    pub fn new() -> Self {
        WorkList {
            stack: VecDeque::with_capacity(STACK_RESERVE_SIZE),
            prefetch_buffer: VecDeque::with_capacity(PREFETCH_BUFFER_SIZE),
        }
    }

    pub fn push(&mut self, pointer: ObjectPointerPointer) {
        self.stack.push_back(pointer);
    }

    pub fn pop(&mut self) -> Option<ObjectPointerPointer> {
        if self.prefetch_buffer.is_empty() {
            self.prefetch(PREFETCH_BUFFER_SIZE);
        }

        if let Some(pointer) = self.prefetch_buffer.pop_front() {
            self.prefetch(1);
            Some(pointer)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn prefetch(&mut self, amount: usize) {
        for _ in 0..amount {
            if let Some(prefetch) = self.stack.pop_front() {
                self.push_to_prefetch_buffer(prefetch);
            } else {
                break;
            }
        }
    }

    #[cfg(feature = "prefetch")]
    #[inline(always)]
    pub fn push_to_prefetch_buffer(&mut self, pointer: ObjectPointerPointer) {
        unsafe {
            intrinsics::prefetch_read_data(pointer.raw, 0);
        }

        self.prefetch_buffer.push_back(pointer);
    }

    #[cfg(not(feature = "prefetch"))]
    #[inline(always)]
    pub fn push_to_prefetch_buffer(&mut self, pointer: ObjectPointerPointer) {
        self.prefetch_buffer.push_back(pointer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_pointer::ObjectPointer;

    fn fake_pointer(address: usize) -> ObjectPointerPointer {
        ObjectPointerPointer {
            raw: address as *const ObjectPointer,
        }
    }

    #[test]
    fn test_push() {
        let mut worklist = WorkList::new();

        worklist.push(fake_pointer(0x4));

        assert_eq!(worklist.stack.len(), 1);
        assert_eq!(worklist.prefetch_buffer.len(), 0);
    }

    #[test]
    fn test_pop_empty() {
        let mut worklist = WorkList::new();

        assert!(worklist.pop().is_none());
    }

    #[test]
    fn test_pop_non_empty() {
        let mut worklist = WorkList::new();

        worklist.push(fake_pointer(0x1));
        worklist.push(fake_pointer(0x2));
        worklist.push(fake_pointer(0x3));

        assert_eq!(worklist.pop().unwrap().raw as usize, 0x1);
        assert_eq!(worklist.prefetch_buffer.len(), 2);
    }

    #[test]
    fn test_prefetch_empty() {
        let mut worklist = WorkList::new();

        worklist.prefetch(5);

        assert!(worklist.prefetch_buffer.is_empty());
    }

    #[test]
    fn test_prefetch_non_empty() {
        let mut worklist = WorkList::new();

        worklist.push(fake_pointer(0x1));
        worklist.push(fake_pointer(0x2));
        worklist.push(fake_pointer(0x3));
        worklist.prefetch(2);

        assert_eq!(worklist.stack.len(), 1);
        assert_eq!(worklist.prefetch_buffer.len(), 2);
    }
}
