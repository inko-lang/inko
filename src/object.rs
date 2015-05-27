use std::collections::HashMap;

pub struct Object<'l> {
    pub instance_variables: HashMap<&'l str, &'l Object<'l>>,
}
