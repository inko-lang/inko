//! Command for building an Inko bytecode image from a source file.
use crate::compiler;
use crate::error::Error;

/// Compiles Inko source code into a bytecode image.
pub fn run(arguments: &[String]) -> Result<i32, Error> {
    compiler::spawn(arguments)
}
