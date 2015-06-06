use compiled_code::CompiledCode;
use register::Register;
use variable_scope::VariableScope;

/// A CallFrame contains information about a location where code is being
/// executed (file, line, etc).
///
/// A CallFrame is also used for storing values and variables for a certain
/// scope. This makes it easy to remove those values again when unwinding the
/// call stack: simply remove the CallFrame and everything else is also removed.
///
pub struct CallFrame {
    /// The name of the CallFrame, usually the same as the method name.
    pub name: String,

    /// The full path to the file being executed.
    pub file: String,

    /// The line number being executed.
    pub line: usize,

    /// An optional parent CallFrame.
    pub parent: Option<Box<CallFrame>>,

    /// Register for storing temporary values.
    pub register: Register,

    /// Storage for local variables.
    pub variables: VariableScope
}

impl CallFrame {
    /// Creates a basic CallFrame with only details such as the name, file and
    /// line number set.
    ///
    /// # Examples
    ///
    ///     let frame = CallFrame::new("(main)", "main.aeon", 1);
    ///
    pub fn new(name: String, file: String, line: usize) -> CallFrame {
        let frame = CallFrame {
            name: name,
            file: file,
            line: line,
            parent: None,
            register: Register::new(),
            variables: VariableScope::new()
        };

        frame
    }

    /// Creates a new CallFrame from a CompiledCode
    pub fn from_code(code: &CompiledCode) -> CallFrame {
        CallFrame::new(code.name.clone(), code.file.clone(), code.line)
    }

    /// Boxes and sets the current frame's parent.
    pub fn set_parent(&mut self, parent: CallFrame) {
        self.parent = Some(Box::new(parent));
    }

    /// Calls the supplied closure for the current and any parent frames.
    ///
    /// The closure takes a single argument: a reference to the CallFrame
    /// currently being processed.
    ///
    /// # Examples
    ///
    ///     some_child_frame.each_frame(|frame| {
    ///         println!("Frame: {}", frame.name);
    ///     });
    ///
    pub fn each_frame<F>(&self, mut closure: F) where F : FnMut(&CallFrame) {
        let mut frame = self;

        closure(frame);

        while frame.parent.is_some() {
            frame = frame.parent.as_ref().unwrap();

            closure(frame);
        }
    }
}
