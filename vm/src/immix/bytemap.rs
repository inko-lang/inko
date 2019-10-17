//! Object And Line Bytemaps
//!
//! Bytemaps are used for marking live objects as well as marking which lines
//! are in use. An ObjectMap is used for marking objects while tracing.
use crate::immix::block::{LINES_PER_BLOCK, OBJECTS_PER_BLOCK};
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicU8, Ordering};

/// A bytemap for marking objects when they are traced.
pub struct ObjectMap {
    values: [AtomicU8; OBJECTS_PER_BLOCK],
}

/// A bytemap for marking lines when tracing objects.
///
/// Line maps cycle between two different mark values every collection, instead
/// of always using "1" to mark an entry. This is necessary as during a
/// collection we want to:
///
/// 1. Be able to determine which lines are live _after_ the collection.
/// 2. Be able to evacuate objects in a bucket _without_ overwriting still live
///    objects.
///
/// Imagine the eden space is collected, survivor space 1 is about half way
/// through its recyclable blocks, and we need to evacuate objects in survivor
/// space 1:
///
///     X = used object slot
///     _ = available object slot
///
///         1      2      3      4      5      6      7     = line number
///     +------------------------------------------------+
///     | XXXX | XXXX | XXXX | XX__ | ____ | XXXX | XXXX |  = block
///     +------------------------------------------------+
///                              ^           ^
///                 A: allocation cursor     B: next full line
///
/// Here "A" indicates where the next object will be allocated into, and "B"
/// indicates the next full line. If we blindly reset the line map, and "B"
/// includes one or more live objects (that we still have to trace through), we
/// would end up overwriting those objects.
///
/// Toggling the mark value between 1 and 2 allows us to prevent this from
/// happening. At collection time we first swap the value, then trace through
/// all objects. At the end of a collection we reset all entries using an old
/// value to 0, and then check what to do with the block.
///
/// We currently store the mark value in the LineMap, which means an additional
/// byte of space is required. Storing this elsewhere would require passing it
/// as an argument to a whole bunch of methods. For a 1GB heap the extra byte
/// (including alignment of 8 bytes) would require 1MB of additional space.
pub struct LineMap {
    values: [AtomicU8; LINES_PER_BLOCK],
    mark_value: u8,
}

pub trait Bytemap {
    fn values(&self) -> &[AtomicU8];
    fn values_mut(&mut self) -> &mut [AtomicU8];

    fn reset(&mut self);

    /// The value to use for marking an entry.
    fn mark_value(&self) -> u8 {
        1
    }

    /// Sets the given index in the bytemap.
    fn set(&mut self, index: usize) {
        self.values()[index].store(self.mark_value(), Ordering::Release);
    }

    /// Unsets the given index in the bytemap.
    fn unset(&mut self, index: usize) {
        self.values()[index].store(0, Ordering::Release);
    }

    /// Returns `true` if a given index is set.
    fn is_set(&self, index: usize) -> bool {
        self.values()[index].load(Ordering::Acquire) > 0
    }

    /// Returns true if the bytemap is empty.
    ///
    /// The number of values in a bytemap is a multiple of 2, and thus a
    /// multiple of the word size of the current architecture. Since we store
    /// bytes in the bytemap, this allows us to read multiple bytes at once.
    /// This in turn allows us to greatly speed up checking if a bytemap is
    /// empty.
    ///
    /// The downside of this is that this method can not be used safely while
    /// the bytemap is also being modified.
    #[cfg_attr(feature = "cargo-clippy", allow(cast_ptr_alignment))]
    fn is_empty(&self) -> bool {
        let mut offset = 0;

        while offset < self.values().len() {
            // The cast to *mut usize here is important so that reads read a
            // single word, not a byte.
            let value = unsafe {
                let ptr = self.values().as_ptr().add(offset) as *const usize;

                *ptr
            };

            if value > 0 {
                return false;
            }

            offset += mem::size_of::<usize>();
        }

        true
    }

    /// Returns the number of indexes set in the bytemap.
    ///
    /// This method can not be used if the bytemap is modified concurrently.
    fn len(&mut self) -> usize {
        let mut amount = 0;

        for value in self.values_mut().iter_mut() {
            if *value.get_mut() > 0 {
                amount += 1;
            }
        }

        amount
    }
}

impl ObjectMap {
    /// Returns a new, empty object bytemap.
    pub fn new() -> ObjectMap {
        let values = [0_u8; OBJECTS_PER_BLOCK];

        ObjectMap {
            values: unsafe { mem::transmute(values) },
        }
    }
}

impl LineMap {
    /// Returns a new, empty line bytemap.
    pub fn new() -> LineMap {
        let values = [0_u8; LINES_PER_BLOCK];

        LineMap {
            values: unsafe { mem::transmute(values) },
            mark_value: 1,
        }
    }

    pub fn swap_mark_value(&mut self) {
        if self.mark_value == 1 {
            self.mark_value = 2;
        } else {
            self.mark_value = 1;
        }
    }

    /// Resets marks from previous marking cycles.
    pub fn reset_previous_marks(&mut self) {
        for index in 0..LINES_PER_BLOCK {
            let current = self.values[index].get_mut();

            if *current != self.mark_value {
                *current = 0;
            }
        }
    }
}

impl Bytemap for ObjectMap {
    #[inline(always)]
    fn values(&self) -> &[AtomicU8] {
        &self.values
    }

    #[inline(always)]
    fn values_mut(&mut self) -> &mut [AtomicU8] {
        &mut self.values
    }

    fn reset(&mut self) {
        unsafe {
            ptr::write_bytes(self.values.as_mut_ptr(), 0, OBJECTS_PER_BLOCK);
        }
    }
}

impl Bytemap for LineMap {
    #[inline(always)]
    fn values(&self) -> &[AtomicU8] {
        &self.values
    }

    #[inline(always)]
    fn values_mut(&mut self) -> &mut [AtomicU8] {
        &mut self.values
    }

    fn mark_value(&self) -> u8 {
        self.mark_value
    }

    fn reset(&mut self) {
        unsafe {
            ptr::write_bytes(self.values.as_mut_ptr(), 0, LINES_PER_BLOCK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_object_map_set() {
        let mut object_map = ObjectMap::new();

        object_map.set(1);

        assert!(object_map.is_set(1));
    }

    #[test]
    fn test_object_map_unset() {
        let mut object_map = ObjectMap::new();

        object_map.set(1);
        object_map.unset(1);

        assert_eq!(object_map.is_set(1), false);
    }

    #[test]
    fn test_object_map_is_empty() {
        let mut object_map = ObjectMap::new();

        assert_eq!(object_map.is_empty(), true);

        object_map.set(1);

        assert_eq!(object_map.is_empty(), false);
    }

    #[test]
    fn test_object_map_reset() {
        let mut object_map = ObjectMap::new();

        object_map.set(1);
        object_map.reset();

        assert_eq!(object_map.is_set(1), false);
    }

    #[test]
    fn test_object_map_len() {
        let mut object_map = ObjectMap::new();

        object_map.set(1);
        object_map.set(3);

        assert_eq!(object_map.len(), 2);
    }

    #[test]
    fn test_object_map_size_of() {
        // This test is put in place to ensure the ObjectMap type doesn't
        // suddenly grow due to some change.
        assert_eq!(size_of::<ObjectMap>(), OBJECTS_PER_BLOCK);
    }

    #[test]
    fn test_line_map_set() {
        let mut line_map = LineMap::new();

        line_map.set(1);

        assert!(line_map.is_set(1));
    }

    #[test]
    fn test_line_map_unset() {
        let mut line_map = LineMap::new();

        line_map.set(1);
        line_map.unset(1);

        assert_eq!(line_map.is_set(1), false);
    }

    #[test]
    fn test_line_map_is_empty() {
        let mut line_map = LineMap::new();

        assert_eq!(line_map.is_empty(), true);

        line_map.set(1);

        assert_eq!(line_map.is_empty(), false);

        line_map.unset(1);
        line_map.set(60);

        assert_eq!(line_map.is_empty(), false);
    }

    #[test]
    fn test_line_map_reset() {
        let mut line_map = LineMap::new();

        line_map.set(1);
        line_map.reset();

        assert_eq!(line_map.is_set(1), false);
    }

    #[test]
    fn test_line_map_len() {
        let mut line_map = LineMap::new();

        line_map.set(1);
        line_map.set(3);

        assert_eq!(line_map.len(), 2);
    }

    #[test]
    fn test_line_map_size_of() {
        // This test is put in place to ensure the LineMap type doesn't suddenly
        // grow due to some change.
        assert_eq!(size_of::<LineMap>(), LINES_PER_BLOCK + 1);
    }
}
