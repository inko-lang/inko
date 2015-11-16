use object::RcObject;

pub type EmptyResult         = Result<(), String>;
pub type IntegerResult       = Result<usize, String>;
pub type OptionIntegerResult = Result<Option<usize>, String>;
pub type OptionObjectResult  = Result<Option<RcObject>, String>;
pub type ObjectResult        = Result<RcObject, String>;
pub type ObjectVecResult     = Result<Vec<RcObject>, String>;
