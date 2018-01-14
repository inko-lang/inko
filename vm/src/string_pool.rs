//! Pooling of string objects to reduce memory usage.
//!
//! A StringPool can be used to map raw strings to their corresponding VM
//! objects. Mapping is done in such a way that the raw string only has to be
//! stored once.

use std::hash::{Hash, Hasher};
use std::collections::HashMap;

use object_pointer::ObjectPointer;

#[derive(Clone, Copy)]
pub struct StringPointer {
    raw: *const String,
}

pub struct StringPool {
    mapping: HashMap<StringPointer, ObjectPointer>,
}

impl StringPointer {
    pub fn new(pointer: &String) -> Self {
        StringPointer {
            raw: pointer as *const String,
        }
    }

    pub fn as_ref(&self) -> &String {
        unsafe { &*self.raw }
    }
}

unsafe impl Send for StringPointer {}
unsafe impl Sync for StringPointer {}

impl PartialEq for StringPointer {
    fn eq(&self, other: &StringPointer) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for StringPointer {}

impl Hash for StringPointer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl StringPool {
    pub fn new() -> Self {
        StringPool {
            mapping: HashMap::new(),
        }
    }

    pub fn get(&self, string: &String) -> Option<ObjectPointer> {
        let pointer = StringPointer::new(string);

        self.mapping.get(&pointer).cloned()
    }

    /// Adds a new string to the string pool.
    ///
    /// This method will panic if the given ObjectPointer does not reside in the
    /// permanent space.
    pub fn add(&mut self, value: ObjectPointer) {
        if !value.is_permanent() {
            panic!("Only permanent objects can be stored in a string pool");
        }

        // Permanent pointers can not outlive a string pool, thus the below is
        // safe.
        let pointer = StringPointer::new(value.string_value().unwrap());

        self.mapping.insert(pointer, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod string_pointer {
        use super::*;
        use std::collections::HashMap;

        #[test]
        fn test_as_ref() {
            let string = "hello".to_string();
            let ptr = StringPointer::new(&string);

            assert_eq!(ptr.as_ref(), &string);
        }

        #[test]
        fn test_eq() {
            let str1 = "hello".to_string();
            let str2 = "hello".to_string();

            let ptr1 = StringPointer::new(&str1);
            let ptr2 = StringPointer::new(&str2);

            assert!(ptr1 == ptr2);
        }

        #[test]
        fn test_hash() {
            let mut map = HashMap::new();
            let string = "hello".to_string();
            let ptr = StringPointer::new(&string);

            map.insert(ptr, 10);

            assert_eq!(map.get(&ptr).unwrap(), &10);
        }
    }

    mod string_pool {
        use super::*;

        use immix::global_allocator::GlobalAllocator;
        use immix::local_allocator::LocalAllocator;
        use immix::permanent_allocator::PermanentAllocator;
        use config::Config;
        use object_value;

        fn allocator() -> Box<PermanentAllocator> {
            let global_alloc = GlobalAllocator::new();

            Box::new(PermanentAllocator::new(global_alloc, false))
        }

        #[test]
        #[should_panic]
        fn test_add_regular() {
            let global_alloc = GlobalAllocator::new();
            let mut alloc = LocalAllocator::new(global_alloc, &Config::new());

            let mut pool = StringPool::new();
            let pointer = alloc.allocate_empty();

            pool.add(pointer);
        }

        #[test]
        fn test_add_permanent() {
            let mut pool = StringPool::new();
            let mut alloc = allocator();

            let pointer = alloc.allocate_without_prototype(
                object_value::string("a".to_string()),
            );

            pool.add(pointer);

            assert!(pool.get(&"a".to_string()).unwrap() == pointer);
        }
    }
}
