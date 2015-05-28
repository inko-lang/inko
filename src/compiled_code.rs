use instruction::Instruction;

pub struct CompiledCode {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub required_arguments: usize,
    pub optional_arguments: usize,
    pub rest_argument: bool,
    pub locals: Vec<String>,
    pub instructions: Vec<Instruction>,

    pub integer_literals: Vec<isize>,
    pub float_literals: Vec<f64>,
    pub string_literals: Vec<String>
}

impl CompiledCode {
    pub fn new(name: String, file: String, line: usize,
               instructions: Vec<Instruction>) -> CompiledCode {
        CompiledCode {
            name: name,
            file: file,
            line: line,
            required_arguments: 0,
            optional_arguments: 0,
            rest_argument: false,
            locals: Vec::new(),
            instructions: instructions,
            integer_literals: Vec::new(),
            float_literals: Vec::new(),
            string_literals: Vec::new()
        }
    }

    pub fn add_integer_literal(&mut self, value: isize) {
        self.integer_literals.push(value);
    }

    pub fn add_float_literal(&mut self, value: f64) {
        self.float_literals.push(value);
    }
}
