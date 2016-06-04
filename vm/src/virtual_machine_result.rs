use object_pointer::ObjectPointer;

pub type EmptyResult         = Result<(), String>;
pub type IntegerResult       = Result<usize, String>;
pub type OptionIntegerResult = Result<Option<usize>, String>;
pub type OptionObjectResult  = Result<Option<ObjectPointer>, String>;
pub type ObjectResult        = Result<ObjectPointer, String>;
pub type ObjectVecResult     = Result<Vec<ObjectPointer>, String>;
