//! Copying Objects
//!
//! The CopyObject trait can be implemented by allocators to support copying of
//! objects into a heap.

use object::Object;
use object_value;
use object_value::ObjectValue;
use object_pointer::ObjectPointer;

pub trait CopyObject: Sized {
    /// Allocates a copied object.
    fn allocate_copy(&mut self, Object) -> ObjectPointer;

    /// Performs a deep copy of `object_ptr`
    ///
    /// The copy of the input object is allocated on the current heap.
    fn copy_object(&mut self, to_copy_ptr: ObjectPointer) -> ObjectPointer {
        if to_copy_ptr.is_permanent() {
            return to_copy_ptr;
        }

        let to_copy = to_copy_ptr.get();

        // Copy over the object value
        let value_copy = match to_copy.value {
            ObjectValue::None => object_value::none(),
            ObjectValue::Integer(num) => object_value::integer(num),
            ObjectValue::Float(num) => object_value::float(num),
            ObjectValue::String(ref string) => {
                object_value::string(*string.clone())
            }
            ObjectValue::Array(ref raw_vec) => {
                let new_map = raw_vec.iter()
                    .map(|val_ptr| self.copy_object(*val_ptr));

                object_value::array(new_map.collect::<Vec<_>>())
            }
            ObjectValue::File(_) => {
                panic!("ObjectValue::File can not be cloned");
            }
            ObjectValue::Error(num) => object_value::error(num),
            ObjectValue::CompiledCode(ref code) => {
                object_value::compiled_code(code.clone())
            }
            ObjectValue::Binding(_) => {
                panic!("ObjectValue::Binding can not be cloned");
            }
        };

        let mut copy = if let Some(proto_ptr) = to_copy.prototype() {
            let proto_copy = self.copy_object(proto_ptr);

            Object::with_prototype(value_copy, proto_copy)
        } else {
            Object::new(value_copy)
        };

        if let Some(header) = to_copy.header() {
            let header_copy = header.copy_to(self);

            copy.set_header(header_copy);
        }

        self.allocate_copy(copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::global_allocator::GlobalAllocator;
    use immix::local_allocator::LocalAllocator;
    use binding::Binding;
    use compiled_code::CompiledCode;
    use object::Object;
    use object_pointer::ObjectPointer;
    use object_value;

    struct DummyAllocator {
        pub allocator: LocalAllocator,
    }

    impl DummyAllocator {
        pub fn new() -> DummyAllocator {
            let global_alloc = GlobalAllocator::without_preallocated_blocks();

            DummyAllocator { allocator: LocalAllocator::new(global_alloc) }
        }
    }

    impl CopyObject for DummyAllocator {
        fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
            self.allocator.allocate_copy(object)
        }
    }

    #[test]
    fn test_copy_none() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy.allocator.allocate_empty();
        let copy = dummy.copy_object(pointer);

        assert!(copy.get().value.is_none());
    }

    #[test]
    fn test_copy_with_prototype() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy.allocator.allocate_empty();
        let proto = dummy.allocator.allocate_empty();

        pointer.get_mut().set_prototype(proto);

        let copy = dummy.copy_object(pointer);

        assert!(copy.get().prototype().is_some());
    }

    #[test]
    fn test_copy_with_header() {
        let mut dummy = DummyAllocator::new();
        let ptr1 = dummy.allocator.allocate_empty();
        let ptr2 = dummy.allocator.allocate_empty();
        let name = dummy.allocator.allocate_empty();

        ptr1.get_mut().add_attribute(name, ptr2);

        let copy = dummy.copy_object(ptr1);

        assert!(copy.get().header().is_some());
    }

    #[test]
    fn test_copy_integer() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy.allocator
            .allocate_without_prototype(object_value::integer(5));

        let copy = dummy.copy_object(pointer);

        assert!(copy.get().value.is_integer());
        assert_eq!(copy.get().value.as_integer().unwrap(), 5);
    }

    #[test]
    fn test_copy_float() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy.allocator
            .allocate_without_prototype(object_value::float(2.5));

        let copy = dummy.copy_object(pointer);

        assert!(copy.get().value.is_float());
        assert_eq!(copy.get().value.as_float().unwrap(), 2.5);
    }

    #[test]
    fn test_copy_string() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy.allocator
            .allocate_without_prototype(object_value::string("a".to_string()));

        let copy = dummy.copy_object(pointer);

        assert!(copy.get().value.is_string());
        assert_eq!(copy.get().value.as_string().unwrap(), &"a".to_string());
    }

    #[test]
    fn test_copy_array() {
        let mut dummy = DummyAllocator::new();
        let ptr1 = dummy.allocator.allocate_empty();
        let ptr2 = dummy.allocator.allocate_empty();
        let array = dummy.allocator
            .allocate_without_prototype(object_value::array(vec![ptr1, ptr2]));

        let copy = dummy.copy_object(array);

        assert!(copy.get().value.is_array());
        assert_eq!(copy.get().value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_copy_error() {
        let mut dummy = DummyAllocator::new();
        let ptr = dummy.allocator
            .allocate_without_prototype(object_value::error(2));

        let copy = dummy.copy_object(ptr);

        assert!(copy.get().value.is_error());
    }

    #[test]
    fn test_copy_compiled_code() {
        let mut dummy = DummyAllocator::new();
        let cc = CompiledCode::with_rc("a".to_string(),
                                       "a".to_string(),
                                       1,
                                       Vec::new());

        let ptr = dummy.allocator
            .allocate_without_prototype(object_value::compiled_code(cc));

        let copy = dummy.copy_object(ptr);

        assert!(copy.get().value.is_compiled_code());
    }

    #[test]
    #[should_panic]
    fn test_copy_binding() {
        let mut dummy = DummyAllocator::new();
        let binding = Binding::new();
        let pointer = dummy.allocator
            .allocate_without_prototype(object_value::binding(binding));

        dummy.copy_object(pointer);
    }
}
