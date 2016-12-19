//! A CompiledCode contains all information required to run a block of code.
//!
//! This includes methods, classes, blocks/closures (e.g. in a language such as
//! Ruby) and so on. Basically anything that should be executed is a
//! CompiledCode.
//!
//! A CompiledCode object should contain everything that is needed to run it
//! including any literals such as integers, floats, strings as well as other
//! metadata such as the amount of required arguments.
//!
//! CompiledCode objects should not be mutated after they have been fully set
//! up. If a method is modified this should result in a completely new
//! CompiledCode replacing the old version instead of patching an existing
//! CompiledCode.

use std::sync::Arc;

use vm::instruction::Instruction;

/// An immutable, reference counted CompiledCode.
pub type RcCompiledCode = Arc<CompiledCode>;

/// Structure for storing compiled code information.
pub struct CompiledCode {
    /// The name of the CompiledCode, usually the method name.
    pub name: String,

    /// The full file path.
    pub file: String,

    /// The starting line number.
    pub line: u32,

    /// The total number of arguments, excluding the rest argument.
    pub arguments: u32,

    /// The amount of required arguments.
    pub required_arguments: u32,

    /// Whether a rest argument is defined.
    pub rest_argument: bool,

    /// List of local variable names.
    pub locals: Vec<String>,

    /// The instructions to execute.
    pub instructions: Vec<Instruction>,

    /// Any literal integers appearing in the source code.
    pub integer_literals: Vec<i64>,

    /// Any literal floats appearing in the source code.
    pub float_literals: Vec<f64>,

    /// Any literal strings appearing in the source code.
    pub string_literals: Vec<String>,

    /// Extra CompiledCode objects to associate with the current one. This can
    /// be used to store CompiledCode objects for every method in a class in the
    /// CompiledCode object of said class.
    pub code_objects: Vec<RcCompiledCode>,
}

unsafe impl Sync for CompiledCode {}

impl CompiledCode {
    /// Creates a basic CompiledCode with a set of instructions. Other data such
    /// as the required arguments and any literals can be added later on.
    ///
    /// # Examples
    ///
    ///     let code = CompiledCode::new("(main)", "test.aeon", 1, vec![...]);
    ///
    pub fn new(name: String,
               file: String,
               line: u32,
               instructions: Vec<Instruction>)
               -> CompiledCode {
        CompiledCode {
            name: name,
            file: file,
            line: line,
            arguments: 0,
            required_arguments: 0,
            rest_argument: false,
            locals: Vec::new(),
            instructions: instructions,
            integer_literals: Vec::new(),
            float_literals: Vec::new(),
            string_literals: Vec::new(),
            code_objects: Vec::new(),
        }
    }

    /// Creates a new reference counted CompiledCode.
    pub fn with_rc(name: String,
                   file: String,
                   line: u32,
                   instructions: Vec<Instruction>)
                   -> RcCompiledCode {
        Arc::new(CompiledCode::new(name, file, line, instructions))
    }

    pub fn integer(&self, index: usize) -> Result<&i64, String> {
        self.integer_literals
            .get(index)
            .ok_or_else(|| format!("Undefined integer literal {}", index))
    }

    pub fn float(&self, index: usize) -> Result<&f64, String> {
        self.float_literals
            .get(index)
            .ok_or_else(|| format!("Undefined float literal {}", index))
    }

    pub fn string(&self, index: usize) -> Result<&String, String> {
        self.string_literals
            .get(index)
            .ok_or_else(|| format!("Undefined string literal {}", index))
    }

    pub fn code_object(&self, index: usize) -> Result<RcCompiledCode, String> {
        self.code_objects
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Undefined code object {}", index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vm::instruction::{Instruction, InstructionType};

    fn new_compiled_code() -> CompiledCode {
        let ins = Instruction::new(InstructionType::Return, vec![0], 1, 1);

        CompiledCode::new("foo".to_string(), "bar.aeon".to_string(), 1, vec![ins])
    }

    #[test]
    fn test_new() {
        let code = new_compiled_code();

        assert_eq!(code.name, "foo".to_string());
        assert_eq!(code.file, "bar.aeon".to_string());
        assert_eq!(code.line, 1);
        assert_eq!(code.instructions.len(), 1);
    }

    #[test]
    fn test_integer_invalid() {
        assert!(new_compiled_code().integer(0).is_err());
    }

    #[test]
    fn test_integer_valid() {
        let mut code = new_compiled_code();

        code.integer_literals.push(10);

        assert!(code.integer(0).is_ok());
        assert_eq!(code.integer(0).unwrap(), &10);
    }

    #[test]
    fn test_float_invalid() {
        assert!(new_compiled_code().float(0).is_err());
    }

    #[test]
    fn test_float_valid() {
        let mut code = new_compiled_code();

        code.float_literals.push(10.5);

        assert!(code.float(0).is_ok());
    }

    #[test]
    fn test_string_invalid() {
        assert!(new_compiled_code().string(0).is_err());
    }

    #[test]
    fn test_string_valid() {
        let mut code = new_compiled_code();

        code.string_literals.push("hello".to_string());

        assert!(code.string(0).is_ok());
        assert_eq!(code.string(0).unwrap(), &"hello".to_string());
    }

    #[test]
    fn test_code_object_invalid() {
        assert!(new_compiled_code().code_object(0).is_err());
    }

    #[test]
    fn test_code_object_valid() {
        let mut code = new_compiled_code();
        let code_rc = Arc::new(new_compiled_code());

        code.code_objects.push(code_rc);

        assert!(code.code_object(0).is_ok());
    }
}
