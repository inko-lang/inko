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
    pub parent: Option<RcObject<'l>>
}

impl<'l> Object<'l> {
    pub fn new(value: ObjectValue<'l>) -> Object<'l> {
        Object {
            instance_variables: HashMap::new(),
            methods: HashMap::new(),
            value: value,
            pinned: false,
            parent: Option::None
        }
    }
}
