use object::RcObject;

const DEFAULT_CAPACITY: usize = 1024;

/// Struct for storing runtime objects.
///
/// Objects stored in a Heap are owned by said heap and use reference counting
/// (using Rc) to allow shared references. Objects should not be shared between
/// threads.
///
pub struct Heap {
    /// Any objects stored on the heap.
    pub objects: Vec<RcObject>
}

impl Heap {
    /// Creates a Heap with a default capacity.
    ///
    /// # Examples
    ///
    ///     let heap = Heap::new();
    ///
    pub fn new() -> Heap {
        Heap::with_capacity(DEFAULT_CAPACITY)
    }

    /// Creates a Heap with a custom capacity.
    ///
    /// # Examples
    ///
    ///     let heap = Heap::with_capacity(2048);
    ///
    pub fn with_capacity(capacity: usize) -> Heap {
        Heap {
            objects: Vec::with_capacity(capacity)
        }
    }

    /// Stores the given Object on the heap.
    pub fn store_object(&mut self, object: RcObject) {
        self.objects.push(object);
    }

    /*
    /// Allocates and stores a native VM Class.
    ///
    /// These classes are pinned to prevent them from being garbage collected.
    pub fn allocate_vm_class(&mut self, name: String,
                                 parent: Option<RcClass>) -> RcClass {
        let klass = Class::with_pinned_rc(Some(name));

        if parent.is_some() {
            klass.borrow_mut().set_parent_class(parent.unwrap());
        }

        self.store_class(klass.clone());

        klass
    }

    /// Allocates a class in the language
    ///
    /// These objects represent classes in the actual language, they are pinned
    /// to prevent garbage collection.
    pub fn allocate_class(&mut self, instance_of: RcClass,
                          actual_class: RcClass) -> RcObject {
        let value  = ObjectValue::Class(actual_class);
        let object = Object::with_rc(instance_of, value);

        object.borrow_mut().pinned = true;

        self.store_object(object.clone());

        object
    }
    */
}
