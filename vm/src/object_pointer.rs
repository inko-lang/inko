/// A wrapper type for global and thread-local objects.

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::ops::Deref;
use object::Object;

pub type RawObjectPointer = *mut Object;
pub type RcRawObjectPointer = Arc<RwLock<RawObjectPointer>>;

/// A wrapper around either a thread-local or global object.
#[derive(Clone)]
pub enum ObjectPointer {
    Global(RcRawObjectPointer),
    Local(RawObjectPointer)
}

unsafe impl Send for ObjectPointer {}
unsafe impl Sync for ObjectPointer {}

/// A wrapper for objects that dereferences into a RawObjectPointer
///
/// Access to global objects is synchronized automatically, local objects are
/// not synchronized.
///
/// Values of this type can be dereferenced into a RawObjectPointer (for both
/// global and local objects) which can then be turned into mutable/immutable
/// Object references.
pub enum ObjectRef<'a> {
    Global(RwLockReadGuard<'a, RawObjectPointer>),
    GlobalMut(RwLockWriteGuard<'a, RawObjectPointer>),
    Local(RawObjectPointer),
}

impl<'a> ObjectRef<'a> {
    /// Dereferences an ObjectRef into an &Object
    pub fn get(&self) -> &Object {
        unsafe { & *(**self as *const Object) }
    }

    /// Dereferences an ObjectRef into an &mut Object
    pub fn get_mut(&self) -> &mut Object {
        unsafe { &mut ***self }
    }
}

impl<'a> Deref for ObjectRef<'a> {
    type Target = RawObjectPointer;

    fn deref(&self) -> &RawObjectPointer {
        match *self {
            ObjectRef::Global(ref ptr)    => ptr,
            ObjectRef::GlobalMut(ref ptr) => ptr,
            ObjectRef::Local(ref ptr)     => ptr
        }
    }
}

impl ObjectPointer {
    pub fn global(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer::Global(Arc::new(RwLock::new(pointer)))
    }

    pub fn local(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer::Local(pointer)
    }

    /// Returns an ObjectReference containing an immutable pointer.
    pub fn get(&self) -> ObjectRef {
        match *self {
            ObjectPointer::Global(ref arc) => {
                ObjectRef::Global(arc.read().unwrap())
            },
            ObjectPointer::Local(ptr) => ObjectRef::Local(ptr)
        }
    }

    /// Returns an ObjectReference containing a mutable pointer.
    pub fn get_mut(&self) -> ObjectRef {
        match *self {
            ObjectPointer::Global(ref arc) => {
                ObjectRef::GlobalMut(arc.write().unwrap())
            },
            ObjectPointer::Local(ptr) => ObjectRef::Local(ptr)
        }
    }
}
