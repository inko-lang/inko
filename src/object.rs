use std::rc::Rc;
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

pub type RcObject<'l> = Rc<Object<'l>>;

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

    pub fn lookup_method(&self, name: &String) -> Option<&CompiledCode> {
        let mut retval: Option<&CompiledCode> = Option::None;

        // Method defined directly on the object
        if self.methods.contains_key(name) {
            retval = self.methods.get(name)
        }

        // Method defined somewhere in the object hierarchy
        else if self.parent.is_some() {
            let mut parent = self.parent.as_ref();

            while parent.is_some() {
                let unwrapped = parent.unwrap();

                if unwrapped.methods.contains_key(name) {
                    retval = unwrapped.methods.get(name);

                    break;
                }

                parent = unwrapped.parent.as_ref();
            }
        }

        retval
    }
}
