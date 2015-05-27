use std::collections::HashMap;

use compiled_code::CompiledCode;

// TODO: use different Object structs instead of smacking all this in Object
pub enum ObjectValue<'l> {
    Integer(isize),
    Float(f64),
    String(String),
    Array(Vec<&'l Object<'l>>)//,
    //Hash(HashMap<&'l Object<'l>, &'l Object<'l>>)
}

pub struct Object<'l> {
    pub instance_variables: HashMap<&'l str, &'l Object<'l>>,
    pub methods: HashMap<&'l str, CompiledCode<'l>>,
    pub value: ObjectValue<'l>,
    pub pinned: bool
}

impl<'l> Object<'l> {
    pub fn new(value: ObjectValue<'l>) -> Object<'l> {
        Object {
            instance_variables: HashMap::new(),
            methods: HashMap::new(),
            value: value,
            pinned: false
        }
    }
}
