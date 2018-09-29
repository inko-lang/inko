//! Sequences of bytecode instructions with associated literal values.
use catch_table::CatchTable;
use deref_pointer::DerefPointer;
use object_pointer::ObjectPointer;
use vm::instruction::Instruction;

/// An immutable, reference counted CompiledCode.
pub type CompiledCodePointer = DerefPointer<CompiledCode>;

/// Structure for storing compiled code information.
pub struct CompiledCode {
    /// The name of the CompiledCode, usually the method name.
    pub name: ObjectPointer,

    /// The full file path.
    pub file: ObjectPointer,

    /// The starting line number.
    pub line: u16,

    /// The names of the arguments, as interned string pointers.
    pub arguments: Vec<ObjectPointer>,

    /// The amount of required arguments.
    pub required_arguments: u8,

    /// Whether a rest argument is defined.
    pub rest_argument: bool,

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
            rest_argument: false,
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
    pub unsafe fn literal(&self, index: usize) -> ObjectPointer {
        *self.literals.get_unchecked(index)
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

    pub fn label_for_number_of_arguments(&self) -> String {
        if self.rest_argument {
            format!("{}+", self.arguments_count())
        } else {
            format!("{}", self.arguments_count())
        }
    }

    pub fn valid_number_of_arguments(&self, given: usize) -> bool {
        let total = self.arguments_count();
        let required = self.required_arguments();

        if given < required {
            return false;
        }

        if given > total && !self.rest_argument {
            return false;
        }

        true
    }

    pub fn number_of_arguments_to_set(&self, given: usize) -> (bool, usize) {
        let total = self.arguments_count();

        let mut to_set = if given <= total { given } else { total };

        if self.rest_argument && to_set > 0 {
            to_set -= 1;
        }

        (self.rest_argument, to_set)
    }

    pub fn argument_position(&self, name: ObjectPointer) -> Option<usize> {
        for (index, arg) in self.arguments.iter().enumerate() {
            if name == *arg {
                return Some(index);
            }
        }

        None
    }

    pub fn rest_argument_index(&self) -> usize {
        self.arguments_count() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;
    use object_pointer::ObjectPointer;
    use vm::instruction::{Instruction, InstructionType};
    use vm::state::{RcState, State};

    fn state() -> RcState {
        State::new(Config::new(), &[])
    }

    fn new_compiled_code(state: &RcState) -> CompiledCode {
        let ins = Instruction::new(InstructionType::Return, vec![0], 1);

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
    fn test_number_of_arguments_to_set_without_rest() {
        let state = state();
        let arg = state.intern_string("foo".to_string());
        let mut code = new_compiled_code(&state);

        code.arguments = vec![arg];

        assert_eq!(code.number_of_arguments_to_set(1), (false, 1));
    }

    #[test]
    fn test_number_of_arguments_to_set_without_rest_with_multiple_arguments() {
        let state = state();
        let arg = state.intern_string("foo".to_string());
        let mut code = new_compiled_code(&state);

        code.arguments = vec![arg, arg];

        assert_eq!(code.number_of_arguments_to_set(2), (false, 2));
    }

    #[test]
    fn test_number_of_arguments_to_set_with_rest() {
        let state = state();
        let arg = state.intern_string("foo".to_string());
        let mut code = new_compiled_code(&state);

        code.rest_argument = true;
        code.arguments = vec![arg];

        assert_eq!(code.number_of_arguments_to_set(1), (true, 0));
    }

    #[test]
    fn test_number_of_arguments_to_set_with_rest_and_multiple_arguments() {
        let state = state();
        let arg = state.intern_string("foo".to_string());
        let mut code = new_compiled_code(&state);

        code.rest_argument = true;
        code.arguments = vec![arg];

        assert_eq!(code.number_of_arguments_to_set(2), (true, 0));
    }
}
