use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::{Config, Output};
use getopts::Options;
use std::path::PathBuf;

const USAGE: &str = "Usage: inko build [OPTIONS] [FILE]

Compile a source file and its dependencies into an executable.

Examples:

    inko build             # Compile src/main.inko
    inko build hello.inko  # Compile the file hello.inko";

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");
    options.optopt(
        "f",
        "format",
        "The output format to use for diagnostics",
        "FORMAT",
    );

    options.optopt(
        "t",
        "target",
        "The target platform to compile for",
        "TARGET",
    );

    options.optopt(
        "o",
        "output",
        "The path to write the executable to",
        "FILE",
    );

    options.optmulti(
        "i",
        "include",
        "A directory to add to the list of source directories",
        "PATH",
    );

    options.optmulti(
        "",
        "opt",
        "The amount of optimisations to apply",
        "none,balanced,aggressive",
    );

    options.optflag("", "static", "Statically link imported C libraries");
    options.optflag("", "dot", "Output the MIR of every module as DOT files");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut config = Config::default();

    if let Some(val) = matches.opt_str("f") {
        config.set_presenter(&val)?;
    }

    if let Some(val) = matches.opt_str("t") {
        config.set_target(&val)?;
    }

    if let Some(val) = matches.opt_str("opt") {
        config.set_opt(&val)?;
    }

    if matches.opt_present("dot") {
        config.dot = true;
    }

    if matches.opt_present("static") {
        config.static_linking = true;
    }

    for path in matches.opt_strs("i") {
        config.sources.push(path.into());
    }

    if let Some(path) = matches.opt_str("o") {
        config.output = Output::Path(PathBuf::from(path));
    }

    let mut compiler = Compiler::new(config);
    let file = matches.free.get(0).map(PathBuf::from);
    let result = compiler.build(file);

    compiler.print_diagnostics();

    match result {
        Ok(_) => Ok(0),
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::generic(msg)),
    }
}
