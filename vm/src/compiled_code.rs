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

use object_pointer::ObjectPointer;
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
    pub line: u16,

    /// The total number of arguments, excluding the rest argument.
    pub arguments: u8,

    /// The amount of required arguments.
    pub required_arguments: u8,

    /// Whether a rest argument is defined.
    pub rest_argument: bool,

    /// The number of local variables defined.
    pub locals: u16,

    /// The instructions to execute.
    pub instructions: Vec<Instruction>,

    /// Any literal integers appearing in the source code.
    pub integer_literals: Vec<ObjectPointer>,

    /// Any literal floats appearing in the source code.
    pub float_literals: Vec<ObjectPointer>,

    /// Any literal strings appearing in the source code.
    pub string_literals: Vec<ObjectPointer>,

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
    ///     let code = CompiledCode::new("(main)", "test.inko", 1, vec![...]);
    ///
    pub fn new(name: String,
               file: String,
               line: u16,
               instructions: Vec<Instruction>)
               -> CompiledCode {
        CompiledCode {
            name: name,
            file: file,
            line: line,
            arguments: 0,
            required_arguments: 0,
            rest_argument: false,
            locals: 0,
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
                   line: u16,
                   instructions: Vec<Instruction>)
                   -> RcCompiledCode {
        Arc::new(CompiledCode::new(name, file, line, instructions))
    }

    pub fn integer(&self, index: usize) -> ObjectPointer {
        self.integer_literals[index]
    }

    pub fn float(&self, index: usize) -> ObjectPointer {
        self.float_literals[index]
    }

    pub fn string(&self, index: usize) -> ObjectPointer {
        self.string_literals[index]
    }

    pub fn code_object(&self, index: usize) -> &RcCompiledCode {
        &self.code_objects[index]
    }

    /// Returns the instruction at the given index, without checking for bounds.
    #[inline(always)]
    pub fn instruction(&self, index: usize) -> &Instruction {
        unsafe { &self.instructions.get_unchecked(index) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use object_pointer::ObjectPointer;
    use config::Config;
    use vm::instruction::{Instruction, InstructionType};
    use vm::state::State;

    fn new_compiled_code() -> CompiledCode {
        let ins = Instruction::new(InstructionType::Return, vec![0], 1);

        CompiledCode::new("foo".to_string(), "bar.inko".to_string(), 1, vec![ins])
    }

    #[test]
    fn test_new() {
        let code = new_compiled_code();

        assert_eq!(code.name, "foo".to_string());
        assert_eq!(code.file, "bar.inko".to_string());
        assert_eq!(code.line, 1);
        assert_eq!(code.instructions.len(), 1);
    }

    #[test]
    #[should_panic]
    fn test_integer_invalid() {
        new_compiled_code().integer(0);
    }

    #[test]
    fn test_integer_valid() {
        let mut code = new_compiled_code();

        code.integer_literals.push(ObjectPointer::integer(10));

        assert!(code.integer(0) == ObjectPointer::integer(10));
    }

    #[test]
    #[should_panic]
    fn test_float_invalid() {
        new_compiled_code().float(0);
    }

    #[test]
    fn test_float_valid() {
        let mut code = new_compiled_code();
        let state = State::new(Config::new());
        let float = state.allocate_permanent_float(10.5);

        code.float_literals.push(float);

        assert_eq!(code.float(0).float_value().unwrap(), 10.5);
    }

    #[test]
    #[should_panic]
    fn test_string_invalid() {
        new_compiled_code().string(0);
    }

    #[test]
    fn test_string_valid() {
        let mut code = new_compiled_code();
        let pointer = ObjectPointer::integer(42);

        code.string_literals.push(pointer);

        assert!(code.string(0) == pointer);
    }

    #[test]
    #[should_panic]
    fn test_code_object_invalid() {
        new_compiled_code().code_object(0);
    }

    #[test]
    fn test_code_object_valid() {
        let mut code = new_compiled_code();
        let code_rc = Arc::new(new_compiled_code());

        code.code_objects.push(code_rc.clone());

        assert_eq!(code.code_object(0).name, code_rc.name);
    }
}
