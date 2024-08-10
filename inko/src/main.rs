#![allow(clippy::assigning_clones)]

mod command;
mod error;
mod http;
mod options;

use crate::command::main;
use std::io::{stdout, IsTerminal as _};
use std::process::exit;

fn main() {
    match main::run() {
        Ok(status) => exit(status),
        Err(err) => {
            if let Some(message) = err.message {
                if stdout().is_terminal() {
                    eprintln!("\x1b[31;1merror:\x1b[0m {}", message);
                } else {
                    eprintln!("error: {}", message);
                }
            }

            exit(err.status);
        }
    }
}
