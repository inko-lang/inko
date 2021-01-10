#[cfg(not(feature = "jemalloc"))]
#[global_allocator]
static A: std::alloc::System = std::alloc::System;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static A: jemallocator::Jemalloc = jemallocator::Jemalloc;

extern crate getopts;
extern crate libinko;

mod command;
mod compiler;
mod config;
mod error;
mod options;
mod tempfile;
mod vm;

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
