//! Command for compiling and running Inko source code or bytecode images.
use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::{Config as CompilerConfig, IMAGE_EXT};
use getopts::{Options, ParsingStyle};
use std::path::PathBuf;
use vm::config::Config;
use vm::image::Image;
use vm::machine::Machine;

const USAGE: &str = "Usage: inko run [OPTIONS] [FILE]

Compile a source file and its dependencies into a bytecode file, then run it.

If the file is a source file (its extension is .inko), the file is first
compiled into a bytecode file. If the file is a bytecode file (its extension is
.ibi), the file is run directly.

Running source files is meant for development and scripting purposes, and comes
with the overhead of having to run the compiler. For production environments
it's best to compile and run your program separately. For example:

    inko build hello.inko -o hello.ibi # Produces ./hello.ibi
    inko run hello.ibi                 # Run the bytecode file

Examples:

    inko run hello.inko    # Compile and runs the file hello.inko
    inko run hello.ibi     # Run the bytecode file directly";

pub fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);

    options.optflag("h", "help", "Shows this help message");

    options.optopt(
        "f",
        "format",
        "The output format to use for diagnostics",
        "FORMAT",
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

    let input = matches.free.get(0);
    let arguments =
        if matches.free.len() > 1 { &matches.free[1..] } else { &[] };

    match input {
        Some(input) if input.ends_with(IMAGE_EXT) => {
            let config = Config::from_env();
            let image = Image::load_file(config, input).map_err(|e| {
                Error::generic(format!(
                    "Failed to parse bytecode image {}: {}",
                    input, e
                ))
            })?;

            return Machine::boot(image, arguments).map_err(Error::generic);
        }
        _ => {}
    }

    let mut config = CompilerConfig::default();

    if let Some(format) = matches.opt_str("f") {
        config.set_presenter(&format)?;
    }

    for path in matches.opt_strs("i") {
        config.sources.add(path.into());
    }

    let mut compiler = Compiler::new(config);
    let result = compiler.compile_to_memory(input.map(PathBuf::from));

    compiler.print_diagnostics();

    // This ensures we don't keep the compiler instance around beyond this
    // point, as we don't need it from now on.
    drop(compiler);

    match result {
        Ok(bytes) => {
            let config = Config::from_env();
            let image = Image::load_bytes(config, bytes).map_err(|e| {
                Error::generic(format!("Failed to parse bytecode image: {}", e))
            })?;

            Machine::boot(image, arguments).map_err(Error::generic)
        }
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::generic(msg)),
    }
}
