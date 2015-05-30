use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

use compiled_code::CompiledCode;

// TODO: use different Object structs instead of smacking all this in Object
pub enum ObjectValue<'l> {
    None,
    Integer(isize),
    Float(f64),
    String(String),
    Array(Vec<RcObject<'l>>)
}

pub type RcObject<'l> = Rc<RefCell<Object<'l>>>;

pub struct Object<'l> {
    pub instance_variables: HashMap<String, RcObject<'l>>,
    pub methods: HashMap<String, CompiledCode>,
    pub value: ObjectValue<'l>,
    pub pinned: bool,
    pub parent: Option<RcObject<'l>>,
    pub method_cache: HashMap<String, &'l CompiledCode>
}

impl<'l> Object<'l> {
    pub fn new(value: ObjectValue<'l>) -> Object<'l> {
        Object {
            instance_variables: HashMap::new(),
            methods: HashMap::new(),
            value: value,
            pinned: false,
            parent: Option::None,
            method_cache: HashMap::new()
        }
    }

    pub fn new_rc(value: ObjectValue<'l>) -> RcObject<'l> {
        Rc::new(RefCell::new(Object::new(value)))
    }

    pub fn lookup_method(&self, name: &String) -> Option<&CompiledCode> {
        let mut retval: Option<&CompiledCode> = Option::None;

        // Method looked up previously and stored in the cache
        if self.method_cache.contains_key(name) {
            // .get() returns a reference, but we already store a reference in
            // the cache, thus we'd get &&CompiledCode instead of &CompiledCode.
            retval = Option::Some(self.method_cache[name])
        }

        // Method defined directly on the object
        else if self.methods.contains_key(name) {
            retval = self.methods.get(name)
        }

        // Method defined somewhere in the object hierarchy
        else if self.parent.is_some() {
            let mut parent = self.parent.as_ref();

            while parent.is_some() {
                let unwrapped = parent.unwrap().borrow();

                if unwrapped.methods.contains_key(name) {
                    retval = unwrapped.methods.get(name);

                    break;
                }

                parent = unwrapped.parent.as_ref();
            }
        }

        if retval.is_some() {
            //self.method_cache.insert(name.clone(), retval.as_ref().unwrap());
        }

        retval
    }
}
