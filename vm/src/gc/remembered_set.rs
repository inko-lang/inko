//! Remembering of mature objects containing pointers to young objects.
use crate::object_pointer::ObjectPointer;
use parking_lot::Mutex;
use std::mem;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// The number of values that can be put into a chunk.
const CHUNK_VALUES: usize = 4;

/// A single chunk of remembered pointers.
pub struct Chunk {
    /// The next chunk in the remembered set.
    next: Option<Box<Chunk>>,

    /// The index for the next value in the chunk.
    index: AtomicUsize,

    /// The values to store in this chunk.
    values: [ObjectPointer; CHUNK_VALUES],
}

impl Chunk {
    fn boxed() -> (Box<Self>, *mut Chunk) {
        let chunk = Chunk {
            next: None,
            index: AtomicUsize::new(0),
            values: [ObjectPointer::null(); CHUNK_VALUES],
        };

        let boxed = Box::new(chunk);
        let ptr = &*boxed as *const _ as *mut _;

        (boxed, ptr)
    }

    /// Remembers a pointer in the current chunk.
    ///
    /// This method returns `true` if the pointer was remembered.
    fn remember(&mut self, value: ObjectPointer) -> bool {
        loop {
            let index = self.index.load(Ordering::Acquire);

            if index == CHUNK_VALUES {
                return false;
            }

            let next = index + 1;

            let result = match self.index.compare_exchange(
                index,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(x) => x,
                Err(x) => x,
            };

            if result == index {
                self.values[index] = value;

                return true;
            }
        }
    }

    /// Remembers a pointer without any synchronisaton.
    unsafe fn remember_fast(&mut self, value: ObjectPointer) -> bool {
        let index = *self.index.get_mut();

        if index == CHUNK_VALUES {
            return false;
        }

        *self.index.get_mut() += 1;
        self.values[index] = value;

        true
    }
}

/// A collection of pointers to mature objects that contain pointers to young
/// objects.
///
/// Values can be added to a remembered set, and an iterator can be obtained to
/// iterate over these values. Removing individual values is not supported,
/// instead one must prune the entire remembered set.
pub struct RememberedSet {
    /// The first chunk in the remembered set.
    head: Box<Chunk>,

    /// A pointer to the last chunk in the remembered set. New values will be
    /// allocated into this chunk.
    tail: AtomicPtr<Chunk>,

    /// A lock used when allocating a new chunk.
    lock: Mutex<()>,
}

impl RememberedSet {
    /// Creates a new remembered set with a single chunk.
    pub fn new() -> RememberedSet {
        let (head, tail) = Chunk::boxed();

        RememberedSet {
            head,
            tail: AtomicPtr::new(tail),
            lock: Mutex::new(()),
        }
    }

    /// Remembers a pointer in the remembered set.
    ///
    /// This method supports concurrent operations and does not require you to
    /// use a lock of sorts.
    pub fn remember(&self, value: ObjectPointer) {
        loop {
            let tail_ptr = self.tail.load(Ordering::Acquire);
            let mut tail = unsafe { &mut *tail_ptr };

            if tail.remember(value) {
                return;
            }

            let _lock = self.lock.lock();

            if self.tail.load(Ordering::Acquire) != tail_ptr {
                continue;
            }

            let (chunk, new_tail_ptr) = Chunk::boxed();

            tail.next = Some(chunk);
            self.tail.store(new_tail_ptr, Ordering::Release);
        }
    }

    /// Remembers a pointer in the remembered set, without synchronisation.
    unsafe fn remember_fast(&mut self, value: ObjectPointer) {
        loop {
            let tail_ptr = *self.tail.get_mut();
            let mut tail = &mut *tail_ptr;

            if tail.remember_fast(value) {
                return;
            }

            let (chunk, new_tail_ptr) = Chunk::boxed();

            tail.next = Some(chunk);
            *self.tail.get_mut() = new_tail_ptr;
        }
    }

    /// Returns an iterator over the pointers in the remembered set.
    ///
    /// This method takes a mutable reference to `self` as iteration can not
    /// take place when the set is modified concurrently.
    pub fn iter(&mut self) -> RememberedSetIterator {
        RememberedSetIterator {
            chunk: &*self.head,
            index: 0,
        }
    }

    /// Prunes the remembered set by removing pointers to unmarked objects.
    pub fn prune(&mut self) {
        let (mut head, tail) = Chunk::boxed();

        // After this `head` is the old head, and `self.head` will be an
        // empty chunk.
        mem::swap(&mut head, &mut self.head);
        *self.tail.get_mut() = tail;

        let mut current = Some(head);

        while let Some(mut chunk) = current {
            for value in &chunk.values {
                if value.is_null() {
                    // Once we encounter a NULL value there can not be any
                    // non-NULL values that follow it.
                    break;
                }

                if !value.is_marked() {
                    // Pointers that are not marked should no longer be
                    // remembered.
                    continue;
                }

                unsafe {
                    self.remember_fast(*value);
                }
            }

            current = chunk.next.take();
        }
    }

    /// Returns `true` if this RememberedSet is empty.
    pub fn is_empty(&self) -> bool {
        self.head.values[0].is_null()
    }
}

pub struct RememberedSetIterator<'a> {
    chunk: &'a Chunk,
    index: usize,
}

impl<'a> Iterator for RememberedSetIterator<'a> {
    type Item = &'a ObjectPointer;

    fn next(&mut self) -> Option<&'a ObjectPointer> {
        if self.index == CHUNK_VALUES {
            if let Some(chunk) = self.chunk.next.as_ref() {
                self.chunk = chunk;
                self.index = 0;
            } else {
                return None;
            }
        }

        let value = &self.chunk.values[self.index];

        if value.is_null() {
            None
        } else {
            self.index += 1;

            Some(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc_without_weak::ArcWithoutWeak;
    use crate::immix::block::Block;
    use crate::object_pointer::ObjectPointer;
    use parking_lot::Mutex;
    use std::mem;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    #[test]
    fn test_remember_single_pointer() {
        let mut rem_set = RememberedSet::new();

        rem_set.remember(ObjectPointer::integer(4));

        let mut iter = rem_set.iter();

        assert!(iter.next() == Some(&ObjectPointer::integer(4)));
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_remember_two_chunks_of_pointers() {
        let mut rem_set = RememberedSet::new();

        for i in 0..8 {
            rem_set.remember(ObjectPointer::integer(i as i64));
        }

        let mut iter = rem_set.iter();

        // We don't use a loop here so that test failures point to the right
        // line.
        assert!(iter.next() == Some(&ObjectPointer::integer(0)));
        assert!(iter.next() == Some(&ObjectPointer::integer(1)));
        assert!(iter.next() == Some(&ObjectPointer::integer(2)));
        assert!(iter.next() == Some(&ObjectPointer::integer(3)));

        assert!(iter.next() == Some(&ObjectPointer::integer(4)));
        assert!(iter.next() == Some(&ObjectPointer::integer(5)));
        assert!(iter.next() == Some(&ObjectPointer::integer(6)));
        assert!(iter.next() == Some(&ObjectPointer::integer(7)));

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_remember_with_threads() {
        // This test is not super accurate, as any race conditions will likely
        // be timing sensitive. However, it's the least we can do without
        // relying on (potentially large) third-party libraries.
        for _ in 0..128 {
            let rem_set = ArcWithoutWeak::new(Mutex::new(RememberedSet::new()));
            let mut threads = Vec::with_capacity(2);
            let wait = ArcWithoutWeak::new(AtomicBool::new(true));

            for _ in 0..2 {
                let rem_set_clone = rem_set.clone();
                let wait_clone = wait.clone();

                threads.push(thread::spawn(move || {
                    while wait_clone.load(Ordering::Relaxed) {
                        // Spin...
                    }

                    for i in 0..4 {
                        rem_set_clone.lock().remember(ObjectPointer::integer(i))
                    }
                }));
            }

            wait.store(false, Ordering::Relaxed);

            for thread in threads {
                thread.join().unwrap();
            }

            assert_eq!(rem_set.lock().iter().count(), 8);

            // 8 values fit in two chunks, and we only allocate the third chunk
            // when reaching value 9.
            assert!(rem_set.lock().head.next.is_some());
            assert!(rem_set.lock().head.next.as_ref().unwrap().next.is_none());
        }
    }

    #[test]
    fn test_chunk_memory_size() {
        assert_eq!(mem::size_of::<Chunk>(), 48);
    }

    #[test]
    fn test_prune_remembered_set() {
        let mut rem_set = RememberedSet::new();
        let mut block = Block::boxed();

        let ptr1 = ObjectPointer::new(block.request_pointer().unwrap());
        let ptr2 = ObjectPointer::new(block.request_pointer().unwrap());
        let ptr3 = ObjectPointer::new(block.request_pointer().unwrap());

        ptr1.mark();
        ptr2.mark();

        rem_set.remember(ptr1);
        rem_set.remember(ptr2);
        rem_set.remember(ptr3);
        rem_set.prune();

        let mut iter = rem_set.iter();

        assert!(iter.next() == Some(&ptr1));
        assert!(iter.next() == Some(&ptr2));
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_prune_remembered_set_with_two_chunks() {
        let mut rem_set = RememberedSet::new();
        let mut block = Block::boxed();

        let ptr1 = ObjectPointer::new(block.request_pointer().unwrap());
        let ptr2 = ObjectPointer::new(block.request_pointer().unwrap());
        let ptr3 = ObjectPointer::new(block.request_pointer().unwrap());
        let ptr4 = ObjectPointer::new(block.request_pointer().unwrap());
        let ptr5 = ObjectPointer::new(block.request_pointer().unwrap());

        ptr1.mark();
        ptr2.mark();
        ptr3.mark();
        ptr4.mark();

        rem_set.remember(ptr1);
        rem_set.remember(ptr2);
        rem_set.remember(ptr3);
        rem_set.remember(ptr4);
        rem_set.remember(ptr5);
        rem_set.prune();

        let mut iter = rem_set.iter();

        assert!(iter.next() == Some(&ptr1));
        assert!(iter.next() == Some(&ptr2));
        assert!(iter.next() == Some(&ptr3));
        assert!(iter.next() == Some(&ptr4));
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_is_empty() {
        let rem_set = RememberedSet::new();

        assert!(rem_set.is_empty());

        rem_set.remember(ObjectPointer::integer(1));

        assert_eq!(rem_set.is_empty(), false);
    }

    #[test]
    fn test_update_forwarded_pointer() {
        let mut rem_set = RememberedSet::new();
        let mut block = Block::boxed();

        let ptr1 = ObjectPointer::new(block.request_pointer().unwrap());
        let ptr2 = ObjectPointer::new(block.request_pointer().unwrap());

        rem_set.remember(ptr1);
        ptr1.get_mut().forward_to(ptr2);

        // The idea of this test is thread A traces through a pointer in the
        // remembered set, then updates it. If we then re-retrieve the same
        // pointer we should get the new value.
        rem_set
            .iter()
            .next()
            .unwrap()
            .pointer()
            .get_mut()
            .resolve_forwarding_pointer();

        assert!(*rem_set.iter().next().unwrap() == ptr2);
    }
}
