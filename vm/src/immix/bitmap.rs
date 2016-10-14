//! Object And Line Bitmaps
//!
//! Bitmaps are used for marking live objects as well as marking which lines are
//! in use. An ObjectMap is used for marking objects and can hold at most 1024
//! entries while a LineMap is used for marking lines and can hold at most 256
//! entries.

#[cfg(all(target_pointer_width = "64"))]
const OBJECT_ENTRIES: usize = 16;

#[cfg(all(target_pointer_width = "64"))]
const LINE_ENTRIES: usize = 4;

#[cfg(all(target_pointer_width = "64"))]
const BITS_PER_INDEX: usize = 64;

#[cfg(all(target_pointer_width = "32"))]
const OBJECT_ENTRIES: usize = 32;

#[cfg(all(target_pointer_width = "32"))]
const LINE_ENTRIES: usize = 8;

#[cfg(all(target_pointer_width = "32"))]
const BITS_PER_INDEX: usize = 32;

/// The maximum value of a single integer in the object bitmap.
const OBJECT_MAX_VALUE: usize = 1 << (1023 % BITS_PER_INDEX);

/// The maximum value of a single integer in the line bitmap.
const LINE_MAX_VALUE: usize = 1 << (127 % BITS_PER_INDEX);

pub struct ObjectMap {
    values: [usize; OBJECT_ENTRIES],
}

pub struct LineMap {
    values: [usize; LINE_ENTRIES],
}

pub trait Bitmap {
    fn set_index_value(&mut self, usize, usize);
    fn get_index_value(&self, usize) -> usize;
    fn values(&self) -> &[usize];
    fn max_value(&self) -> usize;
    fn max_entries(&self) -> usize;

    /// Sets the given index in the bitmap.
    ///
    /// # Examples
    ///
    ///     let mut bitmap = ObjectMap::new();
    ///
    ///     bitmap.set(4);
    fn set(&mut self, index: usize) {
        let slice_idx = index / BITS_PER_INDEX;
        let bit_offset = index % BITS_PER_INDEX;
        let current = self.get_index_value(slice_idx);

        self.set_index_value(slice_idx, current | (1 << bit_offset));
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
        let slice_idx = index / BITS_PER_INDEX;
        let bit_offset = index % BITS_PER_INDEX;
        let current = self.get_index_value(slice_idx);

        self.set_index_value(slice_idx, current & !(1 << bit_offset));
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
        let slice_idx = index / BITS_PER_INDEX;
        let current = self.get_index_value(slice_idx);

        if current > 0 {
            let bit_offset = index % BITS_PER_INDEX;

            (current & (1 << bit_offset)) != 0
        } else {
            false
        }
    }

    /// Returns `true` if the bitmap is full, `false` otherwise
    fn is_full(&self) -> bool {
        for value in self.values().iter() {
            if *value < self.max_value() {
                return false;
            }
        }

        true
    }

    /// Returns true if the bitmap is empty.
    fn is_empty(&self) -> bool {
        for value in self.values().iter() {
            if *value != 0 {
                return false;
            }
        }

        true
    }

    /// Resets the bitmap.
    fn reset(&mut self) {
        for index in 0..self.max_entries() {
            self.set_index_value(index, 0);
        }
    }

    /// The number of indexes set in the bitmap.
    fn len(&self) -> usize {
        let max_index = self.max_entries() * BITS_PER_INDEX;
        let mut count = 0;

        for index in 0..max_index {
            if self.is_set(index) {
                count += 1;
            }
        }

        count
    }
}

impl ObjectMap {
    /// Returns a new, empty object bitmap.
    pub fn new() -> ObjectMap {
        ObjectMap { values: [0; OBJECT_ENTRIES] }
    }
}

impl LineMap {
    /// Returns a new, empty line bitmap.
    pub fn new() -> LineMap {
        LineMap { values: [0; LINE_ENTRIES] }
    }
}

impl Bitmap for ObjectMap {
    fn set_index_value(&mut self, index: usize, value: usize) {
        self.values[index] = value;
    }

    fn get_index_value(&self, index: usize) -> usize {
        self.values[index]
    }

    fn values(&self) -> &[usize] {
        &self.values
    }

    fn max_entries(&self) -> usize {
        OBJECT_ENTRIES
    }

    fn max_value(&self) -> usize {
        OBJECT_MAX_VALUE
    }
}

impl Bitmap for LineMap {
    fn set_index_value(&mut self, index: usize, value: usize) {
        self.values[index] = value;
    }

    fn get_index_value(&self, index: usize) -> usize {
        self.values[index]
    }

    fn values(&self) -> &[usize] {
        &self.values
    }

    fn max_entries(&self) -> usize {
        LINE_ENTRIES
    }

    fn max_value(&self) -> usize {
        LINE_MAX_VALUE
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
    fn test_object_map_is_full() {
        let mut object_map = ObjectMap::new();

        assert_eq!(object_map.is_full(), false);

        object_map.set(1023);

        assert_eq!(object_map.is_full(), false);

        for index in 0..1024 {
            object_map.set(index);
        }

        assert!(object_map.is_full());
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
        assert_eq!(size_of::<ObjectMap>(), 128);
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
    }

    #[test]
    fn test_line_map_is_full() {
        let mut line_map = LineMap::new();

        assert_eq!(line_map.is_full(), false);

        line_map.set(254);

        assert_eq!(line_map.is_full(), false);

        for index in 0..256 {
            line_map.set(index);
        }

        assert!(line_map.is_full());
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
        assert_eq!(size_of::<LineMap>(), 32);
    }
}
