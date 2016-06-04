//! A CallFrame contains information about a location where code is being
//! executed.
//!
//! A CallFrame is also used for storing values and variables for a certain
//! scope. This makes it easy to remove those values again when unwinding the
//! call stack: simply remove the CallFrame and everything else is also removed.

use binding::{Binding, RcBinding};
use compiled_code::RcCompiledCode;
use object_pointer::ObjectPointer;
use register::Register;

/// Structure for storing call frame data.
pub struct CallFrame {
    /// The name of the CallFrame, usually the same as the method name.
    pub name: String,

    /// The full path to the file being executed.
    pub file: String,

    /// The line number being executed.
    pub line: u32,

    /// An optional parent CallFrame.
    pub parent: Option<Box<CallFrame>>,

    /// Register for storing temporary values.
    pub register: Register,

    pub binding: RcBinding
}

impl CallFrame {
    /// Creates a basic CallFrame with only details such as the name, file and
    /// line number set.
    ///
    /// # Examples
    ///
    ///     let frame = CallFrame::new("(main)", "main.aeon", 1);
    ///
    pub fn new(name: String, file: String, line: u32, self_obj: ObjectPointer) -> CallFrame {
        CallFrame {
            name: name,
            file: file,
            line: line,
            parent: None,
            register: Register::new(),
            binding: Binding::new(self_obj)
        }
    }

    /// Creates a new CallFrame from a CompiledCode
    pub fn from_code(code: RcCompiledCode, self_obj: ObjectPointer) -> CallFrame {
        CallFrame::new(code.name.clone(), code.file.clone(), code.line, self_obj)
    }

    pub fn from_code_with_binding(code: RcCompiledCode, binding: RcBinding) -> CallFrame {
        CallFrame {
            name: code.name.clone(),
            file: code.file.clone(),
            line: code.line,
            parent: None,
            register: Register::new(),
            binding: binding
        }
    }

    /// Boxes and sets the current frame's parent.
    pub fn set_parent(&mut self, parent: CallFrame) {
        self.parent = Some(Box::new(parent));
    }

    pub fn parent(&self) -> Option<&Box<CallFrame>> {
        self.parent.as_ref()
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

    pub fn self_object(&self) -> ObjectPointer {
        read_lock!(self.binding).self_object.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiled_code::CompiledCode;
    use heap::Heap;

    #[test]
    fn test_new() {
        let mut heap = Heap::new();
        let obj = heap.allocate_empty_global();

        let frame = CallFrame
            ::new("foo".to_string(), "test.aeon".to_string(), 1, obj);

        assert_eq!(frame.name, "foo".to_string());
        assert_eq!(frame.file, "test.aeon".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_from_code() {
        let mut heap = Heap::new();
        let obj = heap.allocate_empty_global();

        let code = CompiledCode
            ::with_rc("foo".to_string(), "test.aeon".to_string(), 1, vec![]);

        let frame = CallFrame::from_code(code, obj);

        assert_eq!(frame.name, "foo".to_string());
        assert_eq!(frame.file, "test.aeon".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_set_parent() {
        let mut heap = Heap::new();
        let obj = heap.allocate_empty_global();

        let frame1 = CallFrame
            ::new("foo".to_string(), "test.aeon".to_string(), 1, obj.clone());

        let mut frame2 = CallFrame
            ::new("bar".to_string(), "baz.aeon".to_string(), 1, obj);

        frame2.set_parent(frame1);

        assert!(frame2.parent.is_some());
    }

    #[test]
    fn test_each_frame() {
        let mut heap = Heap::new();
        let obj = heap.allocate_empty_global();

        let frame1 = CallFrame
            ::new("foo".to_string(), "test.aeon".to_string(), 1, obj.clone());

        let mut frame2 = CallFrame
            ::new("bar".to_string(), "baz.aeon".to_string(), 1, obj);

        let mut names: Vec<String> = vec![];

        frame2.set_parent(frame1);

        frame2.each_frame(|frame| {
            names.push(frame.name.clone());
        });

        assert_eq!(names[0], "bar".to_string());
        assert_eq!(names[1], "foo".to_string());
    }
}
