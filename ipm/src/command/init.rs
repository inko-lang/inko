use crate::error::Error;
use crate::manifest::MANIFEST_FILE;
use crate::util::usage;
use getopts::Options;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

const USAGE: &str = "ipm init [OPTIONS] [DIR]

Create a new package in an existing directory

Examples:

    ipm init
    ipm init example/";

const TEMPLATE: &str = "\
# This file contains your project's dependencies. For more information, refer
# to TODO";

pub(crate) fn run(args: &[String]) -> Result<(), Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") {
        usage(&options, USAGE);
        return Ok(());
    }

    let dir = if matches.free.is_empty() {
        env::current_dir()?
    } else {
        PathBuf::from(&matches.free[0])
    };

    if !dir.is_dir() {
        fail!("The directory {:?} doesn't exist", dir);
    }

    let path = dir.join(MANIFEST_FILE);

    if path.exists() {
        Ok(())
    } else {
        let mut file = File::create(&path)
            .map_err(|e| error!("Failed to create {:?}: {}", path, e))?;

        file.write_all(TEMPLATE.as_bytes())
            .map_err(|e| error!("Failed to write to {:?}: {}", path, e))
    }
}
