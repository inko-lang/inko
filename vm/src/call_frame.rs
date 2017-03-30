//! A CallFrame contains information about a location where code is being
//! executed.
//!
//! A CallFrame is also used for storing values and variables for a certain
//! scope. This makes it easy to remove those values again when unwinding the
//! call stack: simply remove the CallFrame and everything else is also removed.

use compiled_code::RcCompiledCode;

/// Structure for storing call frame data.
pub struct CallFrame {
    /// The CompiledCode this frame belongs to.
    pub code: RcCompiledCode,

    /// The line number being executed.
    pub line: u16,

    /// An optional parent CallFrame.
    pub parent: Option<Box<CallFrame>>,
}

/// Struct for iterating over all the call frames in a call stack.
pub struct CallFrameIterator<'a> {
    current: Option<&'a CallFrame>,
}

impl CallFrame {
    /// Creates a new CallFrame.
    pub fn new(code: RcCompiledCode, line: u16) -> CallFrame {
        CallFrame {
            code: code,
            line: line,
            parent: None,
        }
    }

    /// Creates a new CallFrame from a CompiledCode
    pub fn from_code(code: RcCompiledCode) -> CallFrame {
        CallFrame::new(code.clone(), code.line)
    }

    pub fn name(&self) -> &String {
        &self.code.name
    }

    pub fn file(&self) -> &String {
        &self.code.file
    }

    pub fn parent(&self) -> Option<&Box<CallFrame>> {
        self.parent.as_ref()
    }

    /// Boxes and sets the current frame's parent.
    pub fn set_parent(&mut self, parent: Box<CallFrame>) {
        self.parent = Some(parent);
    }

    /// Returns an iterator for traversing the call stack, including the current
    /// call frame.
    pub fn call_stack(&self) -> CallFrameIterator {
        CallFrameIterator { current: Some(self) }
    }
}

impl<'a> Iterator for CallFrameIterator<'a> {
    type Item = &'a CallFrame;

    fn next(&mut self) -> Option<&'a CallFrame> {
        if let Some(frame) = self.current {
            if let Some(parent) = frame.parent() {
                self.current = Some(&**parent);
            } else {
                self.current = None;
            }

            return Some(frame);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiled_code::{CompiledCode, RcCompiledCode};

    fn compiled_code() -> RcCompiledCode {
        CompiledCode::with_rc("foo".to_string(),
                              "test.inko".to_string(),
                              1,
                              vec![])
    }

    #[test]
    fn test_new() {
        let code = compiled_code();
        let frame = CallFrame::new(code, 1);

        assert_eq!(frame.name(), &"foo".to_string());
        assert_eq!(frame.file(), &"test.inko".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_from_code() {
        let code = compiled_code();
        let frame = CallFrame::from_code(code);

        assert_eq!(frame.name(), &"foo".to_string());
        assert_eq!(frame.file(), &"test.inko".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_set_parent() {
        let code = compiled_code();
        let frame1 = Box::new(CallFrame::new(code.clone(), 1));
        let mut frame2 = CallFrame::new(code, 1);

        frame2.set_parent(frame1);

        assert!(frame2.parent.is_some());
    }

    #[test]
    fn test_call_stack() {
        let code = compiled_code();
        let frame1 = Box::new(CallFrame::new(code.clone(), 1));
        let mut frame2 = CallFrame::new(code, 2);

        frame2.set_parent(frame1);

        let mut stack = frame2.call_stack();

        let iterator_val1 = stack.next();
        let iterator_val2 = stack.next();

        assert!(iterator_val1.is_some());
        assert!(iterator_val2.is_some());

        assert_eq!(iterator_val1.unwrap().line, 2);
        assert_eq!(iterator_val2.unwrap().line, 1);
    }
}
