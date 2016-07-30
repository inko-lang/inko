use std::mem::transmute;

use immix::bitmap::{Bitmap, ObjectMap};
use immix::block;
use object::Object;
use tagged_pointer::TaggedPointer;

pub type RawObjectPointer = *mut Object;

/// A pointer to an object managed by the GC.
#[derive(Clone, Copy)]
pub struct ObjectPointer {
    pub raw: TaggedPointer<Object>,
}

unsafe impl Send for ObjectPointer {}
unsafe impl Sync for ObjectPointer {}

/// The bit to use for tagging a pointer as an integer.
pub const INTEGER_BIT: usize = 0; // TODO: implement integers

impl ObjectPointer {
    pub fn new(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer { raw: TaggedPointer::new(pointer) }
    }

    /// Creates a new null pointer.
    pub fn null() -> ObjectPointer {
        ObjectPointer { raw: TaggedPointer::null() }
    }

    /// Returns an immutable reference to the Object.
    pub fn get(&self) -> &Object {
        self.raw.as_ref().unwrap()
    }

    /// Returns a mutable reference to the Object.
    pub fn get_mut(&self) -> &mut Object {
        self.raw.as_mut().unwrap()
    }

    /// Returns true if the current pointer is a null pointer.
    pub fn is_null(&self) -> bool {
        self.raw.is_null()
    }

    /// Returns true if the current pointer points to a permanent object.
    pub fn is_permanent(&self) -> bool {
        self.get().generation().is_permanent()
    }

    /// Returns true if the pointer points to a local object.
    pub fn is_local(&self) -> bool {
        !self.is_permanent()
    }

    /// Returns true if the current pointer can be marked by the GC.
    pub fn is_markable(&self) -> bool {
        self.is_local()
    }

    /// Marks the current object.
    pub fn mark(&self) {
        if !self.is_markable() {
            return;
        }

        let mut bitmap = self.mark_bitmap();
        let index = self.mark_bitmap_index();

        if !bitmap.is_set(index) {
            bitmap.set(index);
        }
    }

    /// Returns the mark bitmap to use for this pointer.
    pub fn mark_bitmap(&self) -> &mut ObjectMap {
        unsafe {
            let ptr: *mut ObjectMap = transmute(self.mark_bitmap_address());

            &mut *ptr
        }
    }

    /// Returns the mark bitmap index to use for this pointer.
    pub fn mark_bitmap_index(&self) -> usize {
        let start_addr = self.mark_bitmap_address() + block::LINE_SIZE;
        let offset = self.raw.untagged() as usize - start_addr;

        offset / block::BYTES_PER_OBJECT
    }

    /// Returns the line index of the current pointer.
    pub fn line_index(&self) -> usize {
        (self.raw.untagged() as usize - self.line_address()) / block::LINE_SIZE
    }

    fn mark_bitmap_address(&self) -> usize {
        (self.raw.untagged() as isize & block::OBJECT_BITMAP_MASK) as usize
    }

    fn line_address(&self) -> usize {
        (self.raw.untagged() as isize & block::LINE_BITMAP_MASK) as usize
    }
}

impl PartialEq for ObjectPointer {
    fn eq(&self, other: &ObjectPointer) -> bool {
        self.raw == other.raw
    }
}
