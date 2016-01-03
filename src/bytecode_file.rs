use compiled_code::RcCompiledCode;

pub struct BytecodeFile {
    pub dependencies: Vec<String>,
    pub body: RcCompiledCode
}

impl BytecodeFile {
    pub fn new(dependencies: Vec<String>, body: RcCompiledCode) -> BytecodeFile {
        BytecodeFile { dependencies: dependencies, body: body }
    }
}
