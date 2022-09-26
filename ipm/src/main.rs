#[macro_use]
mod macros;

mod command;
mod error;
mod git;
mod manifest;
mod util;
mod version;

use crate::util::red;
use command::main;
use std::process::exit;

fn main() {
    if let Err(err) = main::run() {
        eprintln!("{} {}", red("error:"), err);
        exit(1);
    }
}
