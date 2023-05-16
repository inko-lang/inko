use crate::error::Error;
use crate::options::print_usage;
use crate::pkg::manifest::MANIFEST_FILE;
use getopts::Options;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

const USAGE: &str = "inko pkg init [OPTIONS] [DIR]

Create a new package in an existing directory

Examples:

    inko pkg init
    inko pkg init example/";

const TEMPLATE: &str = "\
# This file contains your project's dependencies. For more information, refer
# to https://docs.inko-lang.org/manual/latest/getting-started/modules/";

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let dir = matches.free.get(0).map(PathBuf::from).unwrap_or_else(|| {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    if !dir.is_dir() {
        return Err(Error::generic(format!(
            "The directory {:?} doesn't exist",
            dir
        )));
    }

    let path = dir.join(MANIFEST_FILE);

    if path.exists() {
        return Ok(0);
    }

    File::create(&path)
        .and_then(|mut f| f.write_all(TEMPLATE.as_bytes()))
        .map(|_| 0)
        .map_err(|err| {
            Error::generic(format!(
                "Failed to create {}: {}",
                path.display(),
                err
            ))
        })
}
