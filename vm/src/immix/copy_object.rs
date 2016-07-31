//! Copying Objects
//!
//! The CopyObject trait can be implemented by allocators to support copying of
//! objects into a heap.

use immix::allocation_result::AllocationResult;

use object::Object;
use object_value;
use object_value::ObjectValue;
use object_pointer::ObjectPointer;

pub trait CopyObject: Sized {
    /// Allocates a copied object.
    fn allocate_copy(&mut self, Object) -> AllocationResult;

    /// Performs a deep copy of `object_ptr`
    ///
    /// The copy of the input object is allocated on the current heap.
    fn copy_object(&mut self, to_copy_ptr: ObjectPointer) -> AllocationResult {
        if to_copy_ptr.is_permanent() {
            return (to_copy_ptr, false);
        }

        let mut allocated_new = false;
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
                    .map(|val_ptr| {
                        let (copy, alloc_new) = self.copy_object(*val_ptr);

                        reassign_if_true!(allocated_new, alloc_new);

                        copy
                    });

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
            let (proto_copy, alloc_new) = self.copy_object(proto_ptr);

            reassign_if_true!(allocated_new, alloc_new);

            Object::with_prototype(value_copy, proto_copy)
        } else {
            Object::new(value_copy)
        };

        if let Some(header) = to_copy.header() {
            let (header_copy, alloc_new) = header.copy_to(self);

            reassign_if_true!(allocated_new, alloc_new);

            copy.set_header(header_copy);
        }

        let (copy, alloc_new) = self.allocate_copy(copy);

        reassign_if_true!(allocated_new, alloc_new);

        (copy, allocated_new)
    }
}
