//! Object And Line Bytemaps
//!
//! Bytemaps are used for marking live objects as well as marking which lines
//! are in use. An ObjectMap is used for marking objects and can hold at most
//! 1024 entries while a LineMap is used for marking lines and can hold at most
//! 256 entries.
use immix::block::{LINES_PER_BLOCK, OBJECTS_PER_BLOCK};

pub struct ObjectMap {
    values: [u8; OBJECTS_PER_BLOCK],
}

pub struct LineMap {
    values: [u8; LINES_PER_BLOCK],
    mark_value: u8,
}

pub trait Bytemap {
    fn max_entries(&self) -> usize;
    fn values(&self) -> &[u8];
    fn values_mut(&mut self) -> &mut [u8];

    /// Sets the given index in the bitmap.
    ///
    /// # Examples
    ///
    ///     let mut bitmap = ObjectMap::new();
    ///
    ///     bitmap.set(4);
    fn set(&mut self, index: usize) {
        self.values_mut()[index] = 1;
    }

    /// Unsets the given index in the bitmap.
    ///
    /// # Examples
    ///
    ///     let mut bitmap = ObjectMap::new();
    ///
    ///     bitmap.set(4);
    ///     bitmap.unset(4);
    fn unset(&mut self, index: usize) {
        self.values_mut()[index] = 0;
    }

    /// Returns `true` if a given index is set.
    ///
    /// # Examples
    ///
    ///     let mut bitmap = ObjectMap::new();
    ///
    ///     bitmap.is_set(1); // => false
    ///
    ///     bitmap.set(1);
    ///
    ///     bitmap.is_set(1); // => true
    fn is_set(&self, index: usize) -> bool {
        self.values()[index] != 0
    }

    /// Returns true if the bitmap is empty.
    fn is_empty(&self) -> bool {
        for value in self.values().iter() {
            if value != &0 {
                return false;
            }
        }

        true
    }

    /// Resets the bitmap.
    fn reset(&mut self) {
        for index in 0..self.max_entries() {
            self.unset(index);
        }
    }

    /// The number of indexes set in the bitmap.
    fn len(&self) -> usize {
        let mut count = 0;

        for value in self.values().iter() {
            if value != &0 {
                count += 1;
            }
        }

        count
    }
}

impl ObjectMap {
    /// Returns a new, empty object bitmap.
    pub fn new() -> ObjectMap {
        ObjectMap {
            values: [0; OBJECTS_PER_BLOCK],
        }
    }
}

impl LineMap {
    /// Returns a new, empty line bitmap.
    pub fn new() -> LineMap {
        LineMap {
            values: [0; LINES_PER_BLOCK],
            mark_value: 1,
        }
    }

    pub fn set(&mut self, index: usize) {
        self.values[index] = self.mark_value;
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
        for index in 0..self.max_entries() {
            let current = self.values[index];

            if current > 0 && current != self.mark_value {
                self.values[index] = 0;
            }
        }
    }
}

impl Bytemap for ObjectMap {
    #[inline(always)]
    fn values(&self) -> &[u8] {
        &self.values
    }

    #[inline(always)]
    fn values_mut(&mut self) -> &mut [u8] {
        &mut self.values
    }

    fn max_entries(&self) -> usize {
        OBJECTS_PER_BLOCK
    }
}

impl Bytemap for LineMap {
    #[inline(always)]
    fn values(&self) -> &[u8] {
        &self.values
    }

    #[inline(always)]
    fn values_mut(&mut self) -> &mut [u8] {
        &mut self.values
    }

    fn max_entries(&self) -> usize {
        LINES_PER_BLOCK
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
    fn test_line_map_set_swap_marks() {
        let mut line_map = LineMap::new();

        line_map.set(1);
        line_map.swap_mark_value();
        line_map.set(2);

        assert!(line_map.is_set(1));
        assert!(line_map.is_set(2));
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

    #[test]
    fn test_line_map_swap_mark_value() {
        let mut line_map = LineMap::new();

        assert_eq!(line_map.mark_value, 1);

        line_map.swap_mark_value();

        assert_eq!(line_map.mark_value, 2);
    }

    #[test]
    fn test_line_map_reset_previous_marks() {
        let mut line_map = LineMap::new();

        line_map.set(1);
        line_map.set(2);

        line_map.swap_mark_value();

        line_map.set(3);
        line_map.reset_previous_marks();

        assert_eq!(line_map.is_set(1), false);
        assert_eq!(line_map.is_set(2), false);
        assert_eq!(line_map.is_set(3), true);
    }
}
