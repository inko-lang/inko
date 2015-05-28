use std::collections::HashMap;
use object::RcObject;

pub struct ConstantTable<'l> {
    pub constants: HashMap<String, RcObject<'l>>,
    pub parent: Option<&'l ConstantTable<'l>>
}
