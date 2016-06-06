//! Object Metadata
//!
//! The ObjectHeader struct stores metadata associated with an Object, such as
//! the name, attributes, constants and methods. An ObjectHeader struct is only
//! allocated when actually needed.

use std::collections::HashMap;

use heap::Heap;
use object_pointer::ObjectPointer;

macro_rules! has_map_key {
    ($collection: expr, $key: expr) => ({
        if $collection.is_none() {
            return false;
        }

        $collection.as_ref().unwrap().contains_key($key)
    });
}

macro_rules! get_map_key {
    ($collection: expr, $key: expr) => ({
        if $collection.is_none() {
            return None;
        }

        $collection.as_ref().unwrap().get($key).cloned()
    });
}

macro_rules! add_map_key {
    ($collection: expr, $key: expr, $value: expr) => ({
        if $collection.is_none() {
            $collection = Some(Box::new(HashMap::new()));
        }

        $collection.as_mut().unwrap().insert($key, $value);
    });
}

pub type LazyObjectMap = Option<Box<HashMap<String, ObjectPointer>>>;

pub struct ObjectHeader {
    pub attributes: LazyObjectMap,
    pub constants: LazyObjectMap,
    pub methods: LazyObjectMap,

    /// The object to use for constant lookups when a constant is not available
    /// in the prototype hierarchy.
    pub outer_scope: Option<ObjectPointer>
}

impl ObjectHeader {
    pub fn new() -> ObjectHeader {
        ObjectHeader {
            attributes: None,
            constants: None,
            methods: None,
            outer_scope: None
        }
    }

    pub fn copy_to(&self, heap: &mut Heap) -> ObjectHeader {
        let mut copy = ObjectHeader::new();

        if let Some(map) = self.attributes.as_ref() {
            for (key, value) in map.iter() {
                copy.add_attribute(key.clone(),
                                   heap.copy_object(value.clone()));
            }
        }

        if let Some(map) = self.constants.as_ref() {
            for (key, value) in map.iter() {
                copy.add_constant(key.clone(),
                                  heap.copy_object(value.clone()));
            }
        }

        if let Some(map) = self.methods.as_ref() {
            for (key, value) in map.iter() {
                copy.add_method(key.clone(),
                                heap.copy_object(value.clone()));
            }
        }

        if let Some(scope) = self.outer_scope.as_ref() {
            copy.outer_scope = Some(heap.copy_object(scope.clone()));
        }

        copy
    }

    pub fn add_method(&mut self, key: String, value: ObjectPointer) {
        add_map_key!(self.methods, key, value);
    }

    pub fn add_attribute(&mut self, key: String, value: ObjectPointer) {
        add_map_key!(self.attributes, key, value);
    }

    pub fn add_constant(&mut self, key: String, value: ObjectPointer) {
        add_map_key!(self.constants, key, value);
    }

    pub fn get_method(&self, key: &str) -> Option<ObjectPointer> {
        get_map_key!(self.methods, key)
    }

    pub fn get_attribute(&self, key: &str) -> Option<ObjectPointer> {
        get_map_key!(self.attributes, key)
    }

    pub fn get_constant(&self, key: &str) -> Option<ObjectPointer> {
        get_map_key!(self.constants, key)
    }

    pub fn has_method(&self, key: &str) -> bool {
        has_map_key!(self.methods, key)
    }

    pub fn has_constant(&self, key: &str) -> bool {
        has_map_key!(self.constants, key)
    }

    pub fn has_attribute(&self, key: &str) -> bool {
        has_map_key!(self.attributes, key)
    }
}
