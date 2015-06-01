use std::rc::Rc;

use call_frame::CallFrame;
use instruction::Instruction;

/// A reference counteded (using Rc) CompiledCode object.
pub type RcCompiledCode = Rc<CompiledCode>;

/// Enum indicating the visibility of a method
#[derive(Clone)]
pub enum MethodVisibility {
    Public,
    Private
}

/// A CompiledCode contains all information required to run a block of code.
///
/// This includes methods, classes, blocks/closures (e.g. in a language such as
/// Ruby) and so on. Basically anything that should be executed is a
/// CompiledCode.
///
/// A CompiledCode object should contain everything that is needed to run it
/// including any literals such as integers, floats, strings as well as other
/// metadata such as the amount of required arguments.
///
/// CompiledCode objects should not be mutated after they have been fully set
/// up. If a method is modified this should result in a completely new
/// CompiledCode replacing the old version instead of patching an existing
/// CompiledCode.
///
#[derive(Clone)]
pub struct CompiledCode {
    /// The name of the CompiledCode, usually the method name.
    pub name: String,

    /// The full file path.
    pub file: String,

    /// The starting line number.
    pub line: usize,

    /// The amount of required arguments.
    pub required_arguments: usize,

    /// The method visibility (public or private)
    pub visibility: MethodVisibility,

    /// List of local variable names.
    pub locals: Vec<String>,

    /// The instructions to execute.
    pub instructions: Vec<Instruction>,

    /// Any literal integers appearing in the source code.
    pub integer_literals: Vec<isize>,

    /// Any literal floats appearing in the source code.
    pub float_literals: Vec<f64>,

    /// Any literal strings appearing in the source code.
    pub string_literals: Vec<String>,

    /// Extra CompiledCode objects to associate with the current one. This can
    /// be used to store CompiledCode objects for every method in a class in the
    /// CompiledCode object of said class.
    pub code_objects: Vec<CompiledCode>
}

impl CompiledCode {
    /// Creates a basic CompiledCode with a set of instructions. Other data such
    /// as the required arguments and any literals can be added later on.
    ///
    /// # Examples
    ///
    ///     let code = CompiledCode::new("(main)", "test.aeon", 1, vec![...]);
    ///
    pub fn new(name: String, file: String, line: usize,
               instructions: Vec<Instruction>) -> CompiledCode {
        CompiledCode {
            name: name,
            file: file,
            line: line,
            required_arguments: 0,
            visibility: MethodVisibility::Public,
            locals: Vec::new(),
            instructions: instructions,
            integer_literals: Vec::new(),
            float_literals: Vec::new(),
            string_literals: Vec::new(),
            code_objects: Vec::new()
        }
    }

    /// Adds a new integer literal to the current CompiledCode.
    ///
    /// # Examples
    ///
    ///     let mut code = CompiledCode::new(...);
    ///
    ///     code.add_integer_literal(10);
    ///
    pub fn add_integer_literal(&mut self, value: isize) {
        self.integer_literals.push(value);
    }

    /// Adds a new float literal to the current CompiledCode.
    ///
    /// # Examples
    ///
    ///     let mut code = CompiledCode::new(...);
    ///
    ///     code.add_float_literal(10.5);
    ///
    pub fn add_float_literal(&mut self, value: f64) {
        self.float_literals.push(value);
    }

    /// Adds a new string literal to the current CompiledCode.
    ///
    /// # Examples
    ///
    ///     let mut code = CompiledCode::new(...);
    ///
    ///     code.add_string_literal("hello".to_string());
    ///
    pub fn add_string_literal(&mut self, value: String) {
        self.string_literals.push(value);
    }

    /// Creates and returns a CallFrame based on the current CompiledCode.
    pub fn new_call_frame(&self) -> CallFrame {
        CallFrame::new(self.name.clone(), self.file.clone(), self.line)
    }

    /// Returns true for a private CompiledCode
    pub fn is_private(&self) -> bool {
        match self.visibility {
            MethodVisibility::Private => true,
            _                         => false
        }
    }

    /// Returns a reference counted copy of this CompiledCode
    pub fn to_rc(&self) -> RcCompiledCode {
        Rc::new(self.clone())
    }
}
