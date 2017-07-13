extern crate getopts;
extern crate ansi_term;
extern crate xdg;

pub mod macros;

pub mod backend;
pub mod compiler;
pub mod config;
pub mod default_globals;
pub mod diagnostic;
pub mod diagnostics;
pub mod formatter;
pub mod lexer;
pub mod mutability;
pub mod parser;
pub mod rc_cell;
pub mod state;
pub mod symbol;
pub mod symbol_table;
pub mod tir;
pub mod types;

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use formatter::Formatter;
use formatter::pretty::Pretty as PrettyFormatter;

fn print_usage(options: &getopts::Options) -> ! {
    print_stderr(format!("{}", options.usage("Usage: inkoc FILE [OPTIONS]")));

    process::exit(1);
}

fn print_stderr(message: String) {
    let mut stderr = io::stderr();

    stderr.write(message.as_bytes()).unwrap();
    stderr.write(b"\n").unwrap();
    stderr.flush().unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut options = getopts::Options::new();

    options.optflag("h", "help", "Shows this help message");
    options.optflag("v", "version", "Prints the version number");

    options.optmulti(
        "I",
        "include",
        "Directories to search for source files",
        "DIR",
    );

    options.optmulti(
        "T",
        "target",
        "The directory to store compiled bytecode files in",
        "DIR",
    );

    options.optflag("", "release", "Compiles a release build");

    let matches = match options.parse(&args[1..]) {
        Ok(matches) => matches,
        Err(error) => {
            print_stderr(format!("{}", error.to_string()));
            print_usage(&options);
        }
    };

    if matches.opt_present("h") {
        print_usage(&options);
    }

    if matches.opt_present("v") {
        println!("inkoc {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if matches.free.is_empty() {
        print_usage(&options);
    } else {
        let mut config = config::Config::new();

        if let Some(path) = matches.opt_str("T") {
            config.set_target(PathBuf::from(path));
        };

        if matches.opt_present("release") {
            config.set_release_mode();
        }

        if matches.opt_present("I") {
            for dir in matches.opt_strs("I") {
                config.add_source_directory(dir);
            }
        }

        config.create_directories();

        let mut compiler = compiler::Compiler::new(config);

        for path in matches.free.iter() {
            compiler.compile(path.to_string());

            if compiler.has_diagnostics() {
                let formatter = PrettyFormatter::new();

                print_stderr(formatter.format(compiler.diagnostics()));
            }

            if compiler.has_errors() {
                process::exit(1);
            }
        }
    }
}
