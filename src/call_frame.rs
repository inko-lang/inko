use register::Register;

pub struct CallFrame<'l> {
    pub name: &'l str,
    pub file: &'l str,
    pub line: usize,
    pub parent: Option<Box<CallFrame<'l>>>,
    pub register: Register
}

impl<'l> CallFrame<'l> {
    pub fn new(name: &'l str, file: &'l str, line: usize) -> CallFrame<'l> {
        let frame = CallFrame {
            name: name,
            file: file,
            line: line,
            parent: Option::None,
            register: Register::new()
        };

        frame
    }

    pub fn set_parent(&mut self, parent: CallFrame<'l>) {
        self.parent = Option::Some(Box::new(parent));
    }
}
