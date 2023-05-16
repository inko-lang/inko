mod command;
mod error;
mod options;
mod pkg;

use crate::command::main;
use std::process::exit;

fn main() {
    match main::run() {
        Ok(status) => exit(status),
        Err(err) => {
            if let Some(message) = err.message {
                eprintln!("{}", message);
            }

            exit(err.status);
        }
    }
}
