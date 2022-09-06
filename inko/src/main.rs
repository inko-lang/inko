#[cfg(not(feature = "jemalloc"))]
#[global_allocator]
static A: std::alloc::System = std::alloc::System;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static A: jemallocator::Jemalloc = jemallocator::Jemalloc;

mod command;
mod error;
mod options;

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
