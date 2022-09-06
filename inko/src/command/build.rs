//! Command for building an Inko bytecode image from a source file.
use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::Config as CompilerConfig;
use getopts::Options;
use std::path::PathBuf;

const USAGE: &str = "Usage: inko build [OPTIONS] [FILE]

Compile a source file and its dependencies into a bytecode file.

Examples:

    inko build                   # Compile src/main.inko
    inko build hello.inko        # Compile the file hello.inko";

pub fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Shows this help message");

    options.optopt(
        "f",
        "format",
        "The output format to use for diagnostics",
        "FORMAT",
    );

    options.optopt(
        "o",
        "output",
        "The path to write the bytecode file to",
        "FILE",
    );

    options.optmulti(
        "i",
        "include",
        "A directory to add to the list of source directories",
        "PATH",
    );

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut config = CompilerConfig::default();

    if let Some(format) = matches.opt_str("f") {
        config.set_presenter(&format)?;
    }

    for path in matches.opt_strs("i") {
        config.sources.add(path.into());
    }

    if let Some(path) = matches.opt_str("o") {
        config.output = Some(PathBuf::from(path));
    }

    let mut compiler = Compiler::new(config);
    let file = matches.free.get(0).map(PathBuf::from);
    let result = compiler.compile_to_file(file);

    compiler.print_diagnostics();

    match result {
        Ok(_) => Ok(0),
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::generic(msg)),
    }
}
