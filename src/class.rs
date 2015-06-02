use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::RwLock;

use object::RcObject;
use compiled_code::RcCompiledCode;

/// A mutable, reference counted Class.
pub type RcClass = Rc<RefCell<Class>>;

/// Struct for storing information about a Class.
///
/// A Class struct contains information such as the name (if any), the methods,
/// if it's pinned or not, the parent class, etc.
///
/// A Class struct in Rust is not to be confused with a Class in the Aeon
/// language. While the two are related a Class struct is _not_ used to directly
/// represent a Class in the language, instead it's simply a container for the
/// methods and the likes.
///
pub struct Class {
    /// The name of the class.
    pub name: Option<String>,

    /// An optional parent class.
    pub parent_class: Option<RcClass>,

    /// When set to "true" (usually the case) this class won't be GC'd.
    pub pinned: bool,

    /// The methods available to instances of this class.
    pub methods: RwLock<HashMap<String, RcCompiledCode>>,

    /// The constants defined in this class.
    pub constants: RwLock<HashMap<String, RcObject>>
}

impl Class {
    /// Creates a new Class.
    pub fn new(name: Option<String>) -> Class {
        Class {
            name: name,
            parent_class: None,
            pinned: false,
            methods: RwLock::new(HashMap::new()),
            constants: RwLock::new(HashMap::new())
        }
    }

    /// Creates a new mutable, reference counted Class.
    pub fn with_rc(name: Option<String>) -> RcClass {
        Rc::new(RefCell::new(Class::new(name)))
    }

    /// Creates a new mutable, reference counted, pinned Class.
    pub fn with_pinned_rc(name: Option<String>) -> RcClass {
        let mut klass = Class::new(name);

        klass.pinned = true;

        Rc::new(RefCell::new(klass))
    }

    /// Returns the name of this class
    pub fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    /// Sets the parent class.
    pub fn set_parent_class(&mut self, class: RcClass) {
        self.parent_class = Some(class);
    }

    /// Looks up the method for the given name.
    pub fn lookup_method(&self, name: &String) -> Option<RcCompiledCode> {
        let mut retval: Option<RcCompiledCode> = None;

        let methods = self.methods.read().unwrap();

        // Method defined directly on the object
        if methods.contains_key(name) {
            retval = methods.get(name).cloned();
        }

        // Method defined somewhere in the object hierarchy
        else if self.parent_class.is_some() {
            let mut parent = self.parent_class.clone();

            while parent.is_some() {
                let unwrapped      = parent.unwrap();
                let parent_ref     = unwrapped.borrow();
                let parent_methods = parent_ref.methods.read().unwrap();

                if parent_methods.contains_key(name) {
                    retval = parent_methods.get(name).cloned();

                    break;
                }

                parent = parent_ref.parent_class.clone();
            }
        }

        retval
    }

    /// Adds a new method.
    ///
    /// Adding a method is synchronized using a write lock.
    pub fn add_method(&mut self, name: String, code: RcCompiledCode) {
        let mut methods = self.methods.write().unwrap();

        methods.insert(name, code.clone());
    }

    /// Adds a constant.
    pub fn add_constant(&mut self, name: String, value: RcObject) {
        let mut constants = self.constants.write().unwrap();

        constants.insert(name, value);
    }

    /// Adds a class as a constant.
    ///
    /// This requires an RcObject which has an associated RcClass with a name.
    pub fn add_class(&mut self, object: RcObject) {
        let object_ref = object.borrow();
        let class_ref  = object_ref.class.borrow();
        let name       = class_ref.name().unwrap().clone();

        self.add_constant(name, object.clone());
    }

    /// Looks up a constant.
    ///
    /// If a constant is not found in the current class this method will try to
    /// find it in one of the parent classes.
    pub fn lookup_constant(&self, name: &String) -> Option<RcObject> {
        let mut retval: Option<RcObject> = None;

        let constants = self.constants.read().unwrap();

        if constants.contains_key(name) {
            retval = constants.get(name).cloned();
        }

        // Look up the constant in one of the parents.
        else if self.parent_class.is_some() {
            let mut parent = self.parent_class.clone();

            while parent.is_some() {
                let unwrapped        = parent.unwrap();
                let parent_ref       = unwrapped.borrow();
                let parent_constants = parent_ref.constants.read().unwrap();

                if parent_constants.contains_key(name) {
                    retval = parent_constants.get(name).cloned();

                    break;
                }

                parent = parent_ref.parent_class.clone();
            }
        }

        retval
    }
}
