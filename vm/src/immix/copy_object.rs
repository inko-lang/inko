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
