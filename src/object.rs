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

impl ObjectValue {
    /// Returns true if the current value is an ObjectValue::Integer.
    pub fn is_integer(&self) -> bool {
        match *self {
            ObjectValue::Integer(_) => true,
            _                       => false
        }
    }

    /// Returns a wrapped integer or panics.
    pub fn unwrap_integer(&self) -> isize {
        match *self {
            ObjectValue::Integer(val) => val,
            _ => {
                panic!("ObjectValue::unwrap_integer() called on a non integer");
            }
        }
    }
}

/// A mutable, reference counted Object.
pub type RcObject = Rc<RwLock<Object>>;

/// An Object/instance structure, optionally with an associated value.
///
/// An Object in the VM is used to represent an instance of some Class in the
/// Aeon language. For example, the string "foo" is stored as an Object in the
/// VM mapped to the corresponding Class (using the Class struct in the VM).
///
pub struct Object {
    /// The name of the object, used in error messages if present.
    pub name: Option<String>,

    /// The prototype of the object.
    pub prototype: Option<RcObject>,

    /// The attributes of the object.
    pub attributes: HashMap<String, RcObject>,

    /// The constants defined in this object.
    pub constants: HashMap<String, RcObject>,

    /// The methods defined on this object.
    pub methods: HashMap<String, RcCompiledCode>,

    /// A value associated with the object, if any.
    // TODO: use something like a pointer so Object isn't super fat size wise
    pub value: ObjectValue,

    /// When set to "true" this object won't be GC'd.
    pub pinned: bool
}

impl Object {
    /// Creates a regular Object without using an Rc.
    pub fn new(value: ObjectValue) -> Object {
        Object {
            name: None,
            prototype: None,
            attributes: HashMap::new(),
            constants: HashMap::new(),
            methods: HashMap::new(),
            value: value,
            pinned: false
        }
    }

    /// Creates a mutable, reference counted Object.
    pub fn with_rc(value: ObjectValue) -> RcObject {
        Rc::new(RwLock::new(Object::new(value)))
    }

    /// Creates a new Integer object.
    pub fn new_integer(value: isize, prototype: RcObject) -> RcObject {
        let obj = Object::with_rc(ObjectValue::Integer(value));

        obj.write().unwrap().set_prototype(prototype);

        obj
    }

    /// Returns the name of this object.
    pub fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    /// Sets the name of this object.
    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }

    /// Sets the prototype of this object.
    pub fn set_prototype(&mut self, prototype: RcObject) {
        self.prototype = Some(prototype);
    }

    /// Pins the current object.
    pub fn pin(&mut self) {
        self.pinned = true;
    }

    /// Returns an error message for undefined method calls.
    pub fn undefined_method_error(&self, name: &String) -> String {
        if self.name().is_some() {
            format!(
                "Undefined method \"{}\" called on a {}",
                name,
                self.name().unwrap()
            )
        }
        else if self.prototype.is_some() {
            let proto = self.prototype.as_ref().unwrap().read().unwrap();

            proto.undefined_method_error(name)
        }
        else {
            format!("Undefined method \"{}\" called", name)
        }
    }

    /// Returns an error message for private method calls.
    pub fn private_method_error(&self, name: &String) -> String {
        if self.name().is_some() {
            format!(
                "Private method \"{}\" called on a {}",
                name,
                self.name().unwrap()
            )
        }
        else if self.prototype.is_some() {
            let proto = self.prototype.as_ref().unwrap().read().unwrap();

            proto.private_method_error(name)
        }
        else {
            format!("Private method \"{}\" called", name)
        }
    }

    /// Adds a new method.
    pub fn add_method(&mut self, name: String, code: RcCompiledCode) {
        self.methods.insert(name, code.clone());
    }

    /// Looks up the method for the given name.
    pub fn lookup_method(&self, name: &String) -> Option<RcCompiledCode> {
        let mut retval: Option<RcCompiledCode> = None;

        // Method defined directly on the object
        if self.methods.contains_key(name) {
            retval = self.methods.get(name).cloned();
        }

        // Method defined somewhere in the object hierarchy
        else if self.prototype.is_some() {
            let mut parent = self.prototype.clone();

            while parent.is_some() {
                let unwrapped  = parent.unwrap();
                let parent_ref = unwrapped.read().unwrap();

                if parent_ref.methods.contains_key(name) {
                    retval = parent_ref.methods.get(name).cloned();

                    break;
                }

                parent = parent_ref.prototype.clone();
            }
        }

        retval
    }

    /// Adds a constant.
    pub fn add_constant(&mut self, name: String, value: RcObject) {
        self.constants.insert(name, value);
    }

    /// Adds a constant with the same name as the object.
    pub fn add_named_constant(&mut self, value: RcObject) {
        let name = value.read().unwrap().name().unwrap().clone();

        self.add_constant(name, value);
    }

    /// Looks up a constant in the current or a parent object.
    pub fn lookup_constant(&self, name: &String) -> Option<RcObject> {
        let mut retval: Option<RcObject> = None;

        if self.constants.contains_key(name) {
            retval = self.constants.get(name).cloned();
        }

        // Look up the constant in one of the parents.
        else if self.prototype.is_some() {
            let mut parent = self.prototype.clone();

            while parent.is_some() {
                let unwrapped  = parent.unwrap();
                let parent_ref = unwrapped.read().unwrap();

                if parent_ref.constants.contains_key(name) {
                    retval = parent_ref.constants.get(name).cloned();

                    break;
                }

                parent = parent_ref.prototype.clone();
            }
        }

        retval
    }

    /// Adds a new attribute to the object.
    pub fn add_attribute(&mut self, name: String, object: RcObject) {
        self.attributes.insert(name, object);
    }

    /// Returns the attribute for the given name.
    pub fn lookup_attribute(&self, name: &String) -> Option<RcObject> {
        let mut retval: Option<RcObject> = None;

        if self.attributes.contains_key(name) {
            retval = self.attributes.get(name).cloned();
        }

        else if self.prototype.is_some() {
            let mut parent = self.prototype.clone();

            while parent.is_some() {
                let unwrapped  = parent.unwrap();
                let parent_ref = unwrapped.read().unwrap();

                if parent_ref.attributes.contains_key(name) {
                    retval = parent_ref.attributes.get(name).cloned();

                    break;
                }

                parent = parent_ref.prototype.clone();
            }
        }

        retval
    }
}
