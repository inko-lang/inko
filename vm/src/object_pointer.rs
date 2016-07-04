use std::mem::transmute;
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
}

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
        let tagged = tag_pointer(pointer, 0);

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
            ObjectPointerType::Global => untag_pointer(self.ptr, 0),
            ObjectPointerType::Local => self.ptr,
        }
    }

    /// Returns the type of the current pointer.
    pub fn pointer_type(&self) -> ObjectPointerType {
        if bit_is_tagged(self.ptr, 0) {
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
}

impl PartialEq for ObjectPointer {
    fn eq(&self, other: &ObjectPointer) -> bool {
        self.ptr == other.ptr
    }
}
