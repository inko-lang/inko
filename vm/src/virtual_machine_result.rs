use object_pointer::ObjectPointer;
use virtual_machine_error::VirtualMachineError;

pub type EmptyResult = Result<(), VirtualMachineError>;
pub type BooleanResult = Result<bool, VirtualMachineError>;
pub type IntegerResult = Result<Option<usize>, VirtualMachineError>;
pub type ObjectResult = Result<Option<ObjectPointer>, VirtualMachineError>;
pub type ObjectVecResult = Result<Vec<ObjectPointer>, VirtualMachineError>;
