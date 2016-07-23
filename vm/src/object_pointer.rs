use std::mem::transmute;

use immix::bitmap::{Bitmap, ObjectMap};
use immix::block;
use object::Object;

pub type RawObjectPointer = *mut Object;

/// A pointer to an object managed by the GC.
#[derive(Clone, Copy)]
pub struct ObjectPointer {
    pub ptr: RawObjectPointer,
}

unsafe impl Send for ObjectPointer {}
unsafe impl Sync for ObjectPointer {}

/// The type of object pointer
pub enum ObjectPointerType {
    Local,
    Global,
    Integer,
}

/// The bit to use for tagging a pointer as an integer.
pub const INTEGER_BIT: usize = 0;

/// The bit to use for marking a pointer as a global.
pub const GLOBAL_BIT: usize = 1;

/// Tags the given bit in a pointer.
fn tag_pointer(pointer: RawObjectPointer, bit: usize) -> RawObjectPointer {
    unsafe {
        let num: usize = transmute(pointer);

        transmute(num | 1 << bit)
    }
}

/// Returns an ObjectPointer without the given tagged bit.
fn untag_pointer(pointer: RawObjectPointer, bit: usize) -> RawObjectPointer {
    unsafe {
        let num: usize = transmute(pointer);

        transmute(num & !(1 << bit))
    }
}

/// Returns true if the given bit is set.
fn bit_is_tagged(pointer: RawObjectPointer, bit: usize) -> bool {
    let num: usize = unsafe { transmute(pointer) };
    let shifted = 1 << bit;

    (num & shifted) == shifted
}

impl ObjectPointer {
    pub fn new(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer { ptr: pointer }
    }

    /// Creates a global ObjectPointer
    pub fn global(pointer: RawObjectPointer) -> ObjectPointer {
        let tagged = tag_pointer(pointer, GLOBAL_BIT);

        ObjectPointer::new(tagged)
    }

    /// Returns an immutable reference to the Object.
    pub fn get(&self) -> &Object {
        unsafe { self.untagged_pointer().as_ref().unwrap() }
    }

    /// Returns a mutable reference to the Object.
    pub fn get_mut(&self) -> &mut Object {
        unsafe { self.untagged_pointer().as_mut().unwrap() }
    }

    /// Returns an untagged version of the raw pointer.
    pub fn untagged_pointer(&self) -> RawObjectPointer {
        match self.pointer_type() {
            ObjectPointerType::Global => untag_pointer(self.ptr, GLOBAL_BIT),
            _ => self.ptr,
        }
    }

    /// Returns the type of the current pointer.
    pub fn pointer_type(&self) -> ObjectPointerType {
        if bit_is_tagged(self.ptr, INTEGER_BIT) {
            ObjectPointerType::Integer
        } else if bit_is_tagged(self.ptr, GLOBAL_BIT) {
            ObjectPointerType::Global
        } else {
            ObjectPointerType::Local
        }
    }

    /// Returns true if the current pointer is a local pointer.
    pub fn is_global(&self) -> bool {
        match self.pointer_type() {
            ObjectPointerType::Global => true,
            _ => false,
        }
    }

    /// Returns the type of pointer we're dealing with.
    pub fn is_local(&self) -> bool {
        !self.is_global()
    }

    /// Returns true if the current pointer can be marked by the GC.
    pub fn is_markable(&self) -> bool {
        match self.pointer_type() {
            ObjectPointerType::Global => false,
            ObjectPointerType::Integer => false,
            ObjectPointerType::Local => true,
        }
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
        let bitmap_addr = self.mark_bitmap_address();
        let start_addr = bitmap_addr + block::FIRST_OBJECT_BYTE_OFFSET;

        (self.ptr as usize - start_addr) / block::BYTES_PER_OBJECT
    }

    /// Returns the line index of the current pointer.
    pub fn line_index(&self) -> usize {
        (self.ptr as usize - self.line_address()) / block::LINE_SIZE
    }

    fn mark_bitmap_address(&self) -> usize {
        (self.ptr as isize & block::OBJECT_BITMAP_MASK) as usize
    }

    fn line_address(&self) -> usize {
        (self.ptr as isize & block::LINE_BITMAP_MASK) as usize
    }
}

impl PartialEq for ObjectPointer {
    fn eq(&self, other: &ObjectPointer) -> bool {
        self.ptr == other.ptr
    }
}
