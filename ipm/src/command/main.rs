use crate::command::add;
use crate::command::init;
use crate::command::remove;
use crate::command::sync;
use crate::command::update;
use crate::error::Error;
use crate::util::usage;
use getopts::{Options, ParsingStyle};
use std::env::args;

const USAGE: &str = "ipm [OPTIONS] [COMMAND]

ipm is Inko's package manager, used to install and manage dependencies of
your project.

Commands:

    init      Create a new package
    add       Add or update a dependency
    remove    Remove a dependency
    sync      Download and install dependencies
    update    Update all dependencies to the latest version

Examples:

    ipm init
    ipm add gitlab.com/hello/world 1.2.3";

pub(crate) fn run() -> Result<(), Error> {
    let args: Vec<_> = args().collect();
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("h", "help", "Show this help message");
    options.optflag("v", "version", "Print the version number");

    let matches = options.parse(&args[1..])?;

    if matches.opt_present("h") {
        usage(&options, USAGE);
        return Ok(());
    }

    if matches.opt_present("v") {
        println!("ipm version {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    match matches.free.get(0).map(|s| s.as_str()) {
        Some("init") => init::run(&matches.free[1..]),
        Some("add") => add::run(&matches.free[1..]),
        Some("remove") => remove::run(&matches.free[1..]),
        Some("sync") => sync::run(&matches.free[1..]),
        Some("update") => update::run(&matches.free[1..]),
        Some(cmd) => fail!("The command {:?} is invalid", cmd),
        None => sync::run(&[]),
    }
}
