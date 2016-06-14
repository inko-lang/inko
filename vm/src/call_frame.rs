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
    /// The CompiledCode this frame belongs to.
    pub code: RcCompiledCode,

    /// The line number being executed.
    pub line: u32,

    /// An optional parent CallFrame.
    pub parent: Option<Box<CallFrame>>,

    /// Register for storing temporary values.
    pub register: Register,

    pub binding: RcBinding,
}

impl CallFrame {
    /// Creates a new CallFrame.
    pub fn new(code: RcCompiledCode,
               line: u32,
               self_obj: ObjectPointer)
               -> CallFrame {
        CallFrame {
            code: code,
            line: line,
            parent: None,
            register: Register::new(),
            binding: Binding::new(self_obj),
        }
    }

    /// Creates a new CallFrame from a CompiledCode
    pub fn from_code(code: RcCompiledCode, self_obj: ObjectPointer) -> CallFrame {
        CallFrame::new(code.clone(), code.line, self_obj)
    }

    /// Creates a new CallFrame from a CompiledCode and a Binding.
    pub fn from_code_with_binding(code: RcCompiledCode,
                                  binding: RcBinding)
                                  -> CallFrame {
        CallFrame {
            code: code.clone(),
            line: code.line,
            parent: None,
            register: Register::new(),
            binding: binding,
        }
    }

    pub fn name(&self) -> &String {
        &self.code.name
    }

    pub fn file(&self) -> &String {
        &self.code.file
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
    pub fn each_frame<F>(&self, mut closure: F)
        where F: FnMut(&CallFrame)
    {
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
    use binding::Binding;
    use compiled_code::{CompiledCode, RcCompiledCode};
    use heap::Heap;

    fn compiled_code() -> RcCompiledCode {
        CompiledCode::with_rc("foo".to_string(),
                              "test.aeon".to_string(),
                              1,
                              vec![])
    }

    #[test]
    fn test_new() {
        let mut heap = Heap::local();
        let obj = heap.allocate_empty();

        let code = compiled_code();
        let frame = CallFrame::new(code, 1, obj);

        assert_eq!(frame.name(), &"foo".to_string());
        assert_eq!(frame.file(), &"test.aeon".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_from_code() {
        let mut heap = Heap::local();
        let obj = heap.allocate_empty();

        let code = compiled_code();
        let frame = CallFrame::from_code(code, obj);

        assert_eq!(frame.name(), &"foo".to_string());
        assert_eq!(frame.file(), &"test.aeon".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_from_code_with_binding() {
        let mut heap = Heap::local();
        let obj = heap.allocate_empty();

        let binding = Binding::new(obj);
        let code = compiled_code();
        let frame = CallFrame::from_code_with_binding(code, binding);

        assert_eq!(frame.name(), &"foo".to_string());
        assert_eq!(frame.file(), &"test.aeon".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_set_parent() {
        let mut heap = Heap::local();
        let obj = heap.allocate_empty();

        let code = compiled_code();
        let frame1 = CallFrame::new(code.clone(), 1, obj.clone());
        let mut frame2 = CallFrame::new(code, 1, obj);

        frame2.set_parent(frame1);

        assert!(frame2.parent.is_some());
    }

    #[test]
    fn test_each_frame() {
        let mut heap = Heap::local();
        let obj = heap.allocate_empty();

        let code = compiled_code();
        let frame1 = CallFrame::new(code.clone(), 1, obj.clone());
        let mut frame2 = CallFrame::new(code, 1, obj);

        let mut names: Vec<String> = vec![];

        frame2.set_parent(frame1);

        frame2.each_frame(|frame| names.push(frame.name().clone()));

        assert_eq!(names[0], "foo".to_string());
        assert_eq!(names[1], "foo".to_string());
    }
}
