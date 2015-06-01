use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::RwLock;

use compiled_code::RcCompiledCode;

/// Enum for storing different values in an Object.
pub enum ObjectValue {
    None,
    Integer(isize),
    Float(f64),
    String(String),
    Array(Vec<RcObject>)
}

/// A mutable, reference counted Object.
pub type RcObject = Rc<RefCell<Object>>;

/// A generic Object type with an optional value.
///
/// The Object type represents an object in Aeon, be it a class, instance (e.g.
/// integer) or anything in between. Basically if it's exposed to the language
/// it's probably an Object.
///
/// Currently there's a single Object for all possible values that can be stored
/// (using the ObjectValue enum). This is not ideal due to the enum being at
/// least the size of the largest variant. This might change in the future.
///
/// Objects can have instance variables, methods, a parent, etc. Objects can be
/// pinned to prevent garbage collection, this should only be used for global
/// objects such as classes and bootstrapped objects.
///
pub struct Object {
    /// Name of the object
    pub name: String,

    /// The instance variables of the object. These don't use a lock as objects
    /// can't be modified from multiple threads in parallel.
    pub instance_variables: HashMap<String, RcObject>,

    pub methods: RwLock<HashMap<String, RcCompiledCode>>,

    /// A value associated with the object, if any.
    pub value: ObjectValue,

    /// When set to "true" this object won't be GC'd.
    pub pinned: bool,

    /// An optional parent object.
    pub parent: Option<RcObject>
}

impl Object {
    /// Creates a regular Object without using an Rc.
    ///
    /// # Examples
    ///
    ///     let obj = Object::new(ObjectValue::Integer(10));
    ///
    pub fn new(value: ObjectValue) -> Object {
        Object {
            name: "(anonymous object)".to_string(),
            instance_variables: HashMap::new(),
            methods: RwLock::new(HashMap::new()),
            value: value,
            pinned: false,
            parent: None
        }
    }

    /// Creates a mutable, reference counted Object.
    ///
    /// # Examples
    ///
    ///     let obj = Object::with_rc(ObjectValue::Integer(10));
    ///
    pub fn with_rc(value: ObjectValue) -> RcObject {
        Rc::new(RefCell::new(Object::new(value)))
    }

    /// Looks up and caches a method if it exists.
    ///
    /// A method is looked up in 3 steps:
    ///
    /// 1. If it's in the method cache, use it.
    /// 2. If it's not in the cache but defined on the object, use that.
    /// 3. If it's not cached and not defined in the current object walk the
    ///    object hierarchy, if found the method is used.
    ///
    /// Once a method is found it's cached in the method cache to speed up any
    /// following method calls.
    ///
    /// # Examples
    ///
    ///     let obj  = Object::new(ObjectValue::Integer(10));
    ///     let name = "to_s".to_string();
    ///     let code = obj.lookup_method(&name);
    ///
    ///     if code.is_some() {
    ///         ...
    ///     }
    ///
    pub fn lookup_method(&self, name: &String) -> Option<RcCompiledCode> {
        let mut retval: Option<RcCompiledCode> = None;

        let methods = self.methods.read().unwrap();

        // Method defined directly on the object
        if methods.contains_key(name) {
            retval = methods.get(name).cloned();
        }

        // Method defined somewhere in the object hierarchy
        else if self.parent.is_some() {
            let mut parent = self.parent.clone();

            while parent.is_some() {
                let unwrapped      = parent.unwrap();
                let parent_ref     = unwrapped.borrow();
                let parent_methods = parent_ref.methods.read().unwrap();

                if parent_methods.contains_key(name) {
                    retval = parent_methods.get(name).cloned();

                    break;
                }

                parent = parent_ref.parent.clone();
            }
        }

        retval
    }

    /// Adds a new method
    ///
    /// Adding a method is synchronized using a write lock.
    ///
    pub fn add_method(&mut self, name: &String, code: RcCompiledCode) {
        let mut methods = self.methods.write().unwrap();
        let method_name = name.clone();

        methods.insert(method_name, code.clone());
    }

    /// Returns a reference to the object's name.
    pub fn name(&self) -> &String {
        &self.name
    }
}
