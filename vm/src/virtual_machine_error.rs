pub struct VirtualMachineError {
    pub message: String,
    pub line: u32
}

impl VirtualMachineError {
    pub fn new(message: String, line: u32) -> VirtualMachineError {
        VirtualMachineError { message: message, line: line }
    }
}
