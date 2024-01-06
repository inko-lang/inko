use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::{Config, Linker, Output};
use getopts::Options;
use std::path::PathBuf;

const USAGE: &str = "Usage: inko build [OPTIONS] [FILE]

Compile a source file and its dependencies into an executable.

Examples:

    inko build             # Compile src/main.inko
    inko build hello.inko  # Compile the file hello.inko";

enum Timings {
    None,
    Basic,
    Full,
}

impl Timings {
    fn parse(value: &str) -> Option<Timings> {
        match value {
            "basic" => Some(Timings::Basic),
            "full" => Some(Timings::Full),
            _ => None,
        }
    }
}

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

    options.optopt(
        "",
        "opt",
        "The amount of optimisations to apply",
        "none,balanced,aggressive",
    );

    options.optflag("", "static", "Statically link imported C libraries");
    options.optflag("", "dot", "Output the MIR of every module as DOT files");
    options.optflag("", "verify-llvm", "Verify LLVM IR when generating code");
    options.optflag("", "write-llvm", "Write LLVM IR files to disk");
    options.optflagopt(
        "",
        "timings",
        "Display the time spent compiling code",
        "basic,full",
    );

    options.optopt(
        "",
        "threads",
        "The number of threads to use for parallel compilation",
        "NUM",
    );

    options.optopt(
        "",
        "linker",
        "The specific linker to use, instead of detecting the linker automatically",
        "system,lld,mold",
    );

    options.optflag(
        "",
        "disable-incremental",
        "Disables incremental compilation",
    );

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

    if matches.opt_present("verify-llvm") {
        config.verify_llvm = true;
    }

    if matches.opt_present("write-llvm") {
        config.write_llvm = true;
    }

    if matches.opt_present("static") {
        config.static_linking = true;
    }

    for path in matches.opt_strs("i") {
        config.add_source_directory(path.into());
    }

    if let Some(path) = matches.opt_str("o") {
        config.output = Output::Path(PathBuf::from(path));
    }

    if matches.opt_present("disable-incremental") {
        config.incremental = false;
    }

    if let Some(val) = matches.opt_str("threads") {
        match val.parse::<usize>() {
            Ok(0) | Err(_) => {
                return Err(Error::generic(format!(
                    "'{}' isn't a valid number of threads",
                    val
                )));
            }
            Ok(n) => config.threads = n,
        };
    }

    if let Some(val) = matches.opt_str("linker") {
        config.linker = Linker::parse(&val).ok_or_else(|| {
            Error::generic(format!("'{}' isn't a valid linker", val))
        })?;
    }

    let timings = match matches.opt_str("timings") {
        Some(val) => Timings::parse(&val).ok_or_else(|| {
            Error::generic(format!(
                "'{}' is an invalid --timings argument",
                val
            ))
        })?,
        _ if matches.opt_present("timings") => Timings::Basic,
        _ => Timings::None,
    };

    let mut compiler = Compiler::new(config);
    let file = matches.free.get(0).map(PathBuf::from);
    let result = compiler.build(file);

    compiler.print_diagnostics();

    match timings {
        Timings::Basic => compiler.print_timings(),
        Timings::Full => compiler.print_full_timings(),
        _ => {}
    }

    match result {
        Ok(_) => Ok(0),
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::generic(msg)),
    }
}
