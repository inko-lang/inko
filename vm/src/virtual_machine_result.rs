use object_pointer::ObjectPointer;
use virtual_machine_error::VirtualMachineError;

pub type EmptyResult         = Result<(), VirtualMachineError>;
pub type IntegerResult       = Result<usize, VirtualMachineError>;
pub type OptionIntegerResult = Result<Option<usize>, VirtualMachineError>;
pub type OptionObjectResult  = Result<Option<ObjectPointer>, VirtualMachineError>;
pub type ObjectResult        = Result<ObjectPointer, VirtualMachineError>;
pub type ObjectVecResult     = Result<Vec<ObjectPointer>, VirtualMachineError>;
