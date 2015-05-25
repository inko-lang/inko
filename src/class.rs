use std::collections::HashMap;
use compiled_code::CompiledCode;

pub struct Class<'l> {
    name: &'l str,
    parent: Option<&'l Class<'l>>,
    methods: HashMap<&'l str, CompiledCode<'l>>,
    instance_methods: HashMap<&'l str, CompiledCode<'l>>
}
