//! Sequences of bytecode instructions with associated literal values.
use crate::catch_table::CatchTable;
use crate::deref_pointer::DerefPointer;
use crate::object_pointer::ObjectPointer;
use crate::vm::instruction::Instruction;

/// A pointer to a CompiledCode object.
pub type CompiledCodePointer = DerefPointer<CompiledCode>;

/// Structure for storing compiled code information.
pub struct CompiledCode {
    /// The name of the CompiledCode, usually the method name.
    pub name: ObjectPointer,

    /// The full file path.
    pub file: ObjectPointer,

    /// The line number the code object is defined on.
    pub line: u16,

    /// The names of the arguments, as interned string pointers.
    pub arguments: Vec<ObjectPointer>,

    /// The amount of required arguments.
    pub required_arguments: u8,

    /// The number of local variables defined.
    pub locals: u16,

    /// The number of registers required.
    pub registers: u16,

    /// Boolean indicating if this code object captures any variables from its
    /// enclosing scope.
    pub captures: bool,

    /// The instructions to execute.
    pub instructions: Vec<Instruction>,

    /// The literals (e.g. integers or floats) defined in this compiled code
    /// object.
    pub literals: Vec<ObjectPointer>,

    /// Any compiled code objects stored directly inside the current one.
    pub code_objects: Vec<CompiledCode>,

    /// The table to use for catching values.
    pub catch_table: CatchTable,
}

impl CompiledCode {
    /// Creates a basic CompiledCode with a set of instructions. Other data such
    /// as the required arguments and any literals can be added later on.
    pub fn new(
        name: ObjectPointer,
        file: ObjectPointer,
        line: u16,
        instructions: Vec<Instruction>,
    ) -> CompiledCode {
        CompiledCode {
            name,
            file,
            line,
            arguments: Vec::new(),
            required_arguments: 0,
            locals: 0,
            registers: 0,
            captures: false,
            instructions,
            literals: Vec::new(),
            code_objects: Vec::new(),
            catch_table: CatchTable::new(),
        }
    }

    pub fn locals(&self) -> usize {
        self.locals as usize
    }

    #[inline(always)]
    pub unsafe fn literal(&self, index: u16) -> ObjectPointer {
        *self.literals.get_unchecked(index as usize)
    }

    #[inline(always)]
    pub fn code_object(&self, index: usize) -> CompiledCodePointer {
        DerefPointer::new(&self.code_objects[index])
    }

    /// Returns the instruction at the given index, without checking for bounds.
    ///
    /// The instruction is returned as a DerefPointer, allowing it to be used
    /// while also borrowing an ExecutionContext.
    #[inline(always)]
    pub unsafe fn instruction(
        &self,
        index: usize,
    ) -> DerefPointer<Instruction> {
        DerefPointer::new(&self.instructions.get_unchecked(index))
    }

    #[inline(always)]
    pub fn arguments_count(&self) -> usize {
        self.arguments.len()
    }

    #[inline(always)]
    pub fn required_arguments(&self) -> usize {
        self.required_arguments as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::object_pointer::ObjectPointer;
    use crate::vm::instruction::{Instruction, Opcode};
    use crate::vm::state::{RcState, State};
    use std::mem;

    fn state() -> RcState {
        State::with_rc(Config::new(), &[])
    }

    fn new_compiled_code(state: &RcState) -> CompiledCode {
        let ins = Instruction::new(Opcode::Return, [0, 0, 0, 0, 0, 0], 1);

        CompiledCode::new(
            state.intern_string("foo".to_string()),
            state.intern_string("bar.inko".to_string()),
            1,
            vec![ins],
        )
    }

    #[test]
    fn test_new() {
        let state = state();
        let code = new_compiled_code(&state);
        let name = state.intern_string("foo".to_string());
        let file = state.intern_string("bar.inko".to_string());

        assert!(code.name == name);
        assert!(code.file == file);
        assert_eq!(code.line, 1);
        assert_eq!(code.instructions.len(), 1);
    }

    #[test]
    fn test_literal() {
        let state = state();
        let mut code = new_compiled_code(&state);
        let pointer = ObjectPointer::integer(5);

        code.literals.push(pointer);

        assert!(unsafe { code.literal(0) } == pointer);
    }

    #[test]
    #[should_panic]
    fn test_code_object_invalid() {
        let state = state();
        let code = new_compiled_code(&state);

        code.code_object(0);
    }

    #[test]
    fn test_code_object_valid() {
        let state = state();
        let mut code = new_compiled_code(&state);
        let child = new_compiled_code(&state);

        code.code_objects.push(child);

        assert!(code.code_object(0).name == code.name);
    }

    #[test]
    fn test_compiled_code_size() {
        assert_eq!(mem::size_of::<CompiledCode>(), 144);
    }
}
