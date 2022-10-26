//! Types for memory that stays around permanently.
use crate::chunk::Chunk;
use crate::indexes::ClassIndex;
use crate::mem::{
    Array, ByteArray, Class, ClassPointer, Float, Int, Module, ModulePointer,
    Object, Pointer, String as InkoString,
};
use crate::process::Future;
use ahash::AHashMap;
use std::mem::size_of;
use std::ops::Drop;
use std::sync::Mutex;

pub(crate) const INT_CLASS: usize = 0;
pub(crate) const FLOAT_CLASS: usize = 1;
pub(crate) const STRING_CLASS: usize = 2;
pub(crate) const ARRAY_CLASS: usize = 3;
pub(crate) const BOOLEAN_CLASS: usize = 4;
pub(crate) const NIL_CLASS: usize = 5;
pub(crate) const BYTE_ARRAY_CLASS: usize = 6;
pub(crate) const FUTURE_CLASS: usize = 7;

/// The total number of built-in classes.
const BUILTIN_CLASS_COUNT: usize = FUTURE_CLASS + 1;

/// Allocates a new class, returning a tuple containing the owned pointer and a
/// permanent reference pointer.
macro_rules! class {
    ($name: expr, $methods: expr, $size_source: ident) => {{
        Class::alloc($name.to_string(), $methods, size_of::<$size_source>())
    }};
}

macro_rules! get_class {
    ($space: expr, $index: expr) => {
        unsafe { *$space.classes.get($index) }
    };
}

/// The number of methods used for the various built-in classes.
///
/// These counts are used to determine how much memory is needed for allocating
/// the various built-in classes.
#[derive(Default)]
pub(crate) struct MethodCounts {
    pub int_class: u16,
    pub float_class: u16,
    pub string_class: u16,
    pub array_class: u16,
    pub boolean_class: u16,
    pub nil_class: u16,
    pub byte_array_class: u16,
    pub future_class: u16,
}

/// Memory that sticks around for a program's lifetime (aka permanently).
pub(crate) struct PermanentSpace {
    /// All classes defined by the running program.
    ///
    /// Classes are all stored in the same list, making it easy to efficiently
    /// access them from any module; regardless of what module defined the
    /// class.
    ///
    /// We use a Chunk here for the following reasons:
    ///
    /// 1. The list never grows beyond the size specified in the bytecode.
    /// 2. We need to be able to (concurrently set classes in any order while
    ///    parsing bytecode.
    /// 3. We don't want locking. While parsing bytecode we only set values, and
    ///    after parsing bytecode we only read values.
    ///
    /// The first N (see the value of BUILTIN_CLASS_COUNT) fields are reserved
    /// for built-in classes. Some of these values may be left empty to
    /// accomodate for potential future built-in classes.
    classes: Chunk<ClassPointer>,

    /// All modules that are available to the current program.
    modules: Chunk<ModulePointer>,

    /// A map of strings and their heap allocated Inko strings.
    ///
    /// This map is used to ensure that different occurrences of the same string
    /// literal all use the same heap object.
    interned_strings: Mutex<AHashMap<String, Pointer>>,

    /// Permanently allocated objects (excluding classes) to deallocate when the
    /// program terminates.
    ///
    /// This list doesn't include interned strings, as those are stored
    /// separately.
    permanent_objects: Mutex<Vec<Pointer>>,
}

unsafe impl Sync for PermanentSpace {}

impl PermanentSpace {
    pub(crate) fn new(
        modules: u32,
        classes: u32,
        counts: MethodCounts,
    ) -> Self {
        let int_class = class!("Int", counts.int_class, Int);
        let float_class = class!("Float", counts.float_class, Float);
        let str_class = class!("String", counts.string_class, InkoString);
        let ary_class = class!("Array", counts.array_class, Array);
        let bool_class = class!("Bool", counts.boolean_class, Object);
        let nil_class = class!("Nil", counts.nil_class, Object);
        let bary_class =
            class!("ByteArray", counts.byte_array_class, ByteArray);

        let fut_class = class!("Future", counts.future_class, Future);
        let mut classes = Chunk::new(classes as usize + BUILTIN_CLASS_COUNT);
        let modules = Chunk::new(modules as usize);

        unsafe {
            classes.set(INT_CLASS, int_class);
            classes.set(FLOAT_CLASS, float_class);
            classes.set(STRING_CLASS, str_class);
            classes.set(ARRAY_CLASS, ary_class);
            classes.set(BOOLEAN_CLASS, bool_class);
            classes.set(NIL_CLASS, nil_class);
            classes.set(BYTE_ARRAY_CLASS, bary_class);
            classes.set(FUTURE_CLASS, fut_class);
        }

        Self {
            interned_strings: Mutex::new(AHashMap::default()),
            modules,
            classes,
            permanent_objects: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn int_class(&self) -> ClassPointer {
        get_class!(self, INT_CLASS)
    }

    pub(crate) fn float_class(&self) -> ClassPointer {
        get_class!(self, FLOAT_CLASS)
    }

    pub(crate) fn string_class(&self) -> ClassPointer {
        get_class!(self, STRING_CLASS)
    }

    pub(crate) fn array_class(&self) -> ClassPointer {
        get_class!(self, ARRAY_CLASS)
    }

    pub(crate) fn boolean_class(&self) -> ClassPointer {
        get_class!(self, BOOLEAN_CLASS)
    }

    pub(crate) fn nil_class(&self) -> ClassPointer {
        get_class!(self, NIL_CLASS)
    }

    pub(crate) fn byte_array_class(&self) -> ClassPointer {
        get_class!(self, BYTE_ARRAY_CLASS)
    }

    pub(crate) fn future_class(&self) -> ClassPointer {
        get_class!(self, FUTURE_CLASS)
    }

    /// Interns a permanent string.
    ///
    /// If an Inko String has already been allocated for the given Rust String,
    /// the existing Inko String is returned; otherwise a new one is created.
    pub(crate) fn allocate_string(&self, string: String) -> Pointer {
        let mut strings = self.interned_strings.lock().unwrap();

        if let Some(ptr) = strings.get(&string) {
            return *ptr;
        }

        let ptr = InkoString::alloc(self.string_class(), string.clone())
            .as_permanent();

        strings.insert(string, ptr);
        ptr
    }

    pub(crate) fn allocate_int(&self, value: i64) -> Pointer {
        let ptr = Int::alloc(self.int_class(), value);

        if ptr.is_tagged_int() {
            ptr
        } else {
            let ptr = ptr.as_permanent();

            self.permanent_objects.lock().unwrap().push(ptr);
            ptr
        }
    }

    pub(crate) fn allocate_float(&self, value: f64) -> Pointer {
        let ptr = Float::alloc(self.float_class(), value).as_permanent();

        self.permanent_objects.lock().unwrap().push(ptr);
        ptr
    }

    pub(crate) fn allocate_array(&self, value: Vec<Pointer>) -> Pointer {
        let ptr = Array::alloc(self.array_class(), value).as_permanent();

        self.permanent_objects.lock().unwrap().push(ptr);
        ptr
    }

    /// Adds a class in the global class list.
    ///
    /// This method is unsafe because it allows unsynchronised write access to
    /// the class list. In practise this is fine because the bytecode parser
    /// doesn't set the same class twice, and we never resize the class list.
    /// For the sake of clarity we have marked this method as unsafe anyway.
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::cast_ref_to_mut))]
    pub(crate) unsafe fn add_class(
        &self,
        raw_index: u32,
        class: ClassPointer,
    ) -> Result<(), String> {
        let index = raw_index as usize;

        if index >= self.classes.len() {
            return Err(format!(
                "Unable to define class {:?}, as the class index {} is out of bounds",
                class.name,
                index
            ));
        }

        let existing = *self.classes.get(index);

        if !existing.as_ptr().is_null() {
            return Err(format!(
                "Can't store class {:?} in index {}, as it's already used by class {:?}",
                class.name,
                index,
                existing.name
            ));
        }

        let self_mut = &mut *(self as *const _ as *mut PermanentSpace);

        self_mut.classes.set(index, class);
        Ok(())
    }

    /// Adds a module in the global module list.
    ///
    /// This method is unsafe for the same reasons as
    /// `PermanentSpace::add_class()`.
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::cast_ref_to_mut))]
    pub(crate) unsafe fn add_module(
        &self,
        raw_index: u32,
        module: ModulePointer,
    ) -> Result<(), String> {
        let index = raw_index as usize;

        if index >= self.modules.len() {
            return Err(format!(
                "Unable to define module {:?}, as the module index {} is out of bounds",
                module.name(),
                index
            ));
        }

        let existing = *self.modules.get(index);

        if !existing.as_pointer().untagged_ptr().is_null() {
            return Err(format!(
                "Can't store module {:?} in index {}, as it's already used by module {:?}",
                module.name(),
                index,
                existing.name()
            ));
        }

        let self_mut = &mut *(self as *const _ as *mut PermanentSpace);

        self_mut.modules.set(index, module);
        Ok(())
    }

    pub unsafe fn get_class(&self, index: ClassIndex) -> ClassPointer {
        *self.classes.get(index.into())
    }
}

impl Drop for PermanentSpace {
    fn drop(&mut self) {
        unsafe {
            for pointer in self.interned_strings.lock().unwrap().values() {
                InkoString::drop_and_deallocate(*pointer);
            }

            for ptr in self.permanent_objects.lock().unwrap().iter() {
                ptr.free();
            }

            for index in 0..self.modules.len() {
                let ptr = *self.modules.get(index);

                if !ptr.as_pointer().untagged_ptr().is_null() {
                    Module::drop_and_deallocate(ptr);
                }
            }

            for index in 0..self.classes.len() {
                let ptr = *self.classes.get(index);

                if !ptr.as_ptr().is_null() {
                    Class::drop(ptr);
                }
            }
        }

        // The singleton objects can't contain any sub values, so they don't
        // need to be dropped explicitly.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::{
        Array, ByteArray, Float, Int, Object, String as InkoString,
    };
    use crate::process::Future;
    use std::mem::size_of;

    #[test]
    fn test_class_instance_sizes() {
        let perm = PermanentSpace::new(0, 0, MethodCounts::default());

        assert_eq!(perm.int_class().instance_size, size_of::<Int>());
        assert_eq!(perm.float_class().instance_size, size_of::<Float>());
        assert_eq!(perm.string_class().instance_size, size_of::<InkoString>());
        assert_eq!(perm.array_class().instance_size, size_of::<Array>());
        assert_eq!(perm.boolean_class().instance_size, size_of::<Object>());
        assert_eq!(perm.nil_class().instance_size, size_of::<Object>());
        assert_eq!(
            perm.byte_array_class().instance_size,
            size_of::<ByteArray>()
        );
        assert_eq!(perm.future_class().instance_size, size_of::<Future>());
    }
}
