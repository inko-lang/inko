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
    options.optflag(
        "d",
        "dependencies",
        "Also generate documentation for dependencies",
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

    let mut compiler = Compiler::new(config);
    let conf = compiler::docs::Config {
        private: matches.opt_present("p"),
        dependencies: matches.opt_present("d"),
    };
    let result = compiler.document(conf);

    compiler.print_diagnostics();

    match result {
        Ok(_) => Ok(0),
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::from(msg)),
    }
}
