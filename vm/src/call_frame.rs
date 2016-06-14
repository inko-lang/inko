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
    pub line: u32,

    /// An optional parent CallFrame.
    pub parent: Option<Box<CallFrame>>,
}

impl CallFrame {
    /// Creates a new CallFrame.
    pub fn new(code: RcCompiledCode, line: u32) -> CallFrame {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiled_code::{CompiledCode, RcCompiledCode};

    fn compiled_code() -> RcCompiledCode {
        CompiledCode::with_rc("foo".to_string(),
                              "test.aeon".to_string(),
                              1,
                              vec![])
    }

    #[test]
    fn test_new() {
        let code = compiled_code();
        let frame = CallFrame::new(code, 1);

        assert_eq!(frame.name(), &"foo".to_string());
        assert_eq!(frame.file(), &"test.aeon".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_from_code() {
        let code = compiled_code();
        let frame = CallFrame::from_code(code);

        assert_eq!(frame.name(), &"foo".to_string());
        assert_eq!(frame.file(), &"test.aeon".to_string());
        assert_eq!(frame.line, 1);
    }

    #[test]
    fn test_set_parent() {
        let code = compiled_code();
        let frame1 = CallFrame::new(code.clone(), 1);
        let mut frame2 = CallFrame::new(code, 1);

        frame2.set_parent(frame1);

        assert!(frame2.parent.is_some());
    }

    #[test]
    fn test_each_frame() {
        let code = compiled_code();
        let frame1 = CallFrame::new(code.clone(), 1);
        let mut frame2 = CallFrame::new(code, 1);

        let mut names: Vec<String> = vec![];

        frame2.set_parent(frame1);

        frame2.each_frame(|frame| names.push(frame.name().clone()));

        assert_eq!(names[0], "foo".to_string());
        assert_eq!(names[1], "foo".to_string());
    }
}
