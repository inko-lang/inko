//! Immix Lines
//!
//! In Immix a line is 128 bytes of memory that can be used for storing objects.
//! Since Rust doesn't allow one to easily allocate a chunk of memory and use
//! this for initializing objects we instead use a fixed number of objects per
//! line. Since every Object is 32 bytes we allow a total of 4 per line
//! resulting 128 bytes being used for objects.

use std::mem::size_of;
use object::Object;
use object_pointer::{ObjectPointer, RawObjectPointer};

/// The number of bytes in single line.
pub const LINE_SIZE: usize = 128;

/// Structure representing a line.
///
/// Because we're using a Vec here instead of a regular slice this structure
/// will use slightly more than 128 bytes of memory.
pub struct Line {
    pub objects: Vec<Option<Object>>,
}

impl Line {
    pub fn new() -> Line {
        let capacity = LINE_SIZE / size_of::<Object>();

        Line { objects: Vec::with_capacity(capacity) }
    }

    /// Allocates an Object into this line.
    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
        self.objects.push(Some(object));

        let index = self.objects.len() - 1;

        let raw_pointer =
            self.objects[index].as_mut().unwrap() as RawObjectPointer;

        ObjectPointer::new(raw_pointer)
    }

    /// Returns true if the current line has space available for an object.
    pub fn is_available(&self) -> bool {
        self.objects.len() < self.objects.capacity()
    }
}
