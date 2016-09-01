use std::mem::transmute;
use std::hash::{Hash, Hasher};

use immix::bitmap::{Bitmap, ObjectMap};
use immix::block;
use immix::local_allocator::YOUNG_MAX_AGE;

use object::Object;
use tagged_pointer::TaggedPointer;

pub type RawObjectPointer = *mut Object;

/// A pointer to an object managed by the GC.
#[derive(Clone, Copy)]
pub struct ObjectPointer {
    /// The underlying tagged pointer. This pointer can have the following last
    /// two bits set:
    ///
    ///     00: the pointer is a regular pointer
    ///     01: the pointer is a tagged integer
    ///     10: the pointer is a forwarding pointer
    pub raw: TaggedPointer<Object>,
}

unsafe impl Send for ObjectPointer {}
unsafe impl Sync for ObjectPointer {}

/// The mask to use for tagging a pointer as an integer.
pub const INTEGER_MARK: usize = 0x1; // TODO: implement integers

/// The mask to use for forwarding pointers
pub const FORWARDING_MASK: usize = 0x2;

impl ObjectPointer {
    pub fn new(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer { raw: TaggedPointer::new(pointer) }
    }

    /// Creates a new null pointer.
    pub fn null() -> ObjectPointer {
        ObjectPointer { raw: TaggedPointer::null() }
    }

    /// Returns a forwarding pointer to the current pointer.
    pub fn forwarding_pointer(&self) -> ObjectPointer {
        let raw = TaggedPointer::with_mask(self.raw.raw, FORWARDING_MASK);

        ObjectPointer { raw: raw }
    }

    /// Returns true if the current pointer points to a forwarded object.
    pub fn is_forwarded(&self) -> bool {
        let object = self.get();

        if let Some(proto) = object.prototype() {
            proto.raw.mask_is_set(FORWARDING_MASK)
        } else {
            false
        }
    }

    /// Replaces the current pointer with a pointer to the forwarded object.
    pub fn resolve_forwarding_pointer(&self) {
        let object = self.get();

        if let Some(proto) = object.prototype() {
            // Since object pointers are _usually_ immutable we have to use an
            // extra layer of indirection to update "self".
            unsafe {
                let self_ptr = self as *const ObjectPointer as *mut ObjectPointer;
                let mut self_ref = &mut *self_ptr;

                self_ref.raw = proto.raw.without_tags();
            };
        }
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

    /// Returns true if the current pointer points to a mature object.
    pub fn is_mature(&self) -> bool {
        self.get().generation().is_mature()
    }

    /// Returns true if the current pointer points to a young object.
    pub fn is_young(&self) -> bool {
        self.get().generation().is_young()
    }

    /// Returns true if the pointer points to a local object.
    pub fn is_local(&self) -> bool {
        !self.is_permanent()
    }

    /// Returns true if the current pointer can be marked by the GC.
    pub fn is_markable(&self) -> bool {
        self.is_local()
    }

    /// Returns true if the current object is marked.
    pub fn is_marked(&self) -> bool {
        if !self.is_markable() {
            return false;
        }

        let bitmap = self.mark_bitmap();
        let index = self.mark_bitmap_index();

        bitmap.is_set(index)
    }

    /// Returns true if the underlying object should be promoted to the mature
    /// generation.
    pub fn should_promote_to_mature(&self) -> bool {
        let block_age = self.block().bucket().unwrap().age;

        self.is_young() && block_age >= YOUNG_MAX_AGE && !self.is_forwarded()
    }

    /// Marks the line this object resides in.
    pub fn mark_line(&self) {
        if !self.is_markable() {
            return;
        }

        let mut block = self.block_mut();
        let line_index = self.line_index();

        block.used_lines.set(line_index);
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
        &mut self.block_mut().mark_bitmap
    }

    /// Returns the mark bitmap index to use for this pointer.
    pub fn mark_bitmap_index(&self) -> usize {
        let start_addr = self.block_header_pointer_address();
        let offset = self.raw.untagged() as usize - start_addr;

        offset / block::BYTES_PER_OBJECT
    }

    /// Returns the line index of the current pointer.
    pub fn line_index(&self) -> usize {
        (self.raw.untagged() as usize - self.line_address()) / block::LINE_SIZE
    }

    /// Returns a mutable reference to the block this pointer belongs to.
    pub fn block_mut(&self) -> &mut block::Block {
        self.block_header().block_mut()
    }

    /// Returns an immutable reference to the block this pointer belongs to.
    pub fn block(&self) -> &block::Block {
        self.block_header().block()
    }

    pub fn block_header(&self) -> &block::BlockHeader {
        unsafe {
            let ptr: *mut block::BlockHeader =
                transmute(self.block_header_pointer_address());

            &*ptr
        }
    }

    fn block_header_pointer_address(&self) -> usize {
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

impl Eq for ObjectPointer {}

impl Hash for ObjectPointer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}
