//! Object And Line Bitmaps
//!
//! Bitmaps are used for marking live objects as well as marking which lines are
//! in use. An ObjectMap is used for marking objects and can hold at most 1024
//! entries while a LineMap is used for marking lines and can hold at most 256
//! entries.

/// The number of entries in an object map.
const OBJECT_ENTRIES: usize = 1024;

/// The number of entries in a line map.
const LINE_ENTRIES: usize = 256;

pub struct ObjectMap {
    values: [bool; OBJECT_ENTRIES],
}

pub struct LineMap {
    values: [bool; LINE_ENTRIES],
}

pub trait Bitmap {
    fn max_entries(&self) -> usize;
    fn values(&self) -> &[bool];
    fn values_mut(&mut self) -> &mut [bool];

    /// Sets the given index in the bitmap.
    ///
    /// # Examples
    ///
    ///     let mut bitmap = ObjectMap::new();
    ///
    ///     bitmap.set(4);
    fn set(&mut self, index: usize) {
        self.values_mut()[index] = true;
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
        self.values_mut()[index] = false;
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
        self.values()[index]
    }

    /// Returns `true` if the bitmap is full, `false` otherwise
    fn is_full(&self) -> bool {
        let empty = false;

        for value in self.values().iter() {
            if value == &empty {
                return false;
            }
        }

        true
    }

    /// Returns true if the bitmap is empty.
    fn is_empty(&self) -> bool {
        let set = true;

        for value in self.values().iter() {
            if value == &set {
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
        let set = true;
        let mut count = 0;

        for value in self.values().iter() {
            if value == &set {
                count += 1;
            }
        }

        count
    }
}

impl ObjectMap {
    /// Returns a new, empty object bitmap.
    pub fn new() -> ObjectMap {
        ObjectMap { values: [false; OBJECT_ENTRIES] }
    }
}

impl LineMap {
    /// Returns a new, empty line bitmap.
    pub fn new() -> LineMap {
        LineMap { values: [false; LINE_ENTRIES] }
    }
}

impl Bitmap for ObjectMap {
    fn values(&self) -> &[bool] {
        &self.values
    }

    fn values_mut(&mut self) -> &mut [bool] {
        &mut self.values
    }

    fn max_entries(&self) -> usize {
        OBJECT_ENTRIES
    }
}

impl Bitmap for LineMap {
    fn values(&self) -> &[bool] {
        &self.values
    }

    fn values_mut(&mut self) -> &mut [bool] {
        &mut self.values
    }

    fn max_entries(&self) -> usize {
        LINE_ENTRIES
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
        assert_eq!(size_of::<ObjectMap>(), 1024);
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
        assert_eq!(size_of::<LineMap>(), 256);
    }
}
