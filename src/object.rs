use std::collections::HashMap;
use class::Class;

pub struct Object<'l> {
    instance_variables: HashMap<&'l str, &'l Object<'l>>,
    class: &'l Class<'l>
}
