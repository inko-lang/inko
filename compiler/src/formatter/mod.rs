pub mod pretty;

use diagnostics::Diagnostics;

pub trait Formatter {
    /// Formats all the compiler messages and returns a String containing the
    /// end result.
    fn format(&self, &Diagnostics) -> String;
}
