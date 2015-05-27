use std::collections::HashMap;
use object::Object;

pub struct ConstantTable<'l> {
    pub constants: HashMap<&'l str, &'l Object<'l>>,
    pub parent: Option<&'l ConstantTable<'l>>
}
