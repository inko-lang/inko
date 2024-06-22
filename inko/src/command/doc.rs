use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::Config;
use getopts::Options;

const USAGE: &str = "inko doc [OPTIONS]

Generate JSON files to be used for generating documentation.

The output is a collection of JSON files containing information about the
modules of a project, such as the types and methods they define along with their
documentation. These files can then be used by third-party tools to generate
websites, manual pages, and so on.

Examples:

    inko doc    # Generates documentation for the current project";

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
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
    options.optmulti(
        "i",
        "include",
        "A directory to add to the list of source directories",
        "PATH",
    );
    options.optflag(
        "p",
        "private",
        "Generate documentation for private symbols",
    );

    let matches = options.parse(args)?;

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

    let mut compiler = Compiler::new(config);
    let result = compiler.document(matches.opt_present("p"));

    compiler.print_diagnostics();

    match result {
        Ok(_) => Ok(0),
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::from(msg)),
    }
}
