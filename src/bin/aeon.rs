extern crate libaeon;
extern crate getopts;

use std::io::prelude::*;
use std::env;
use std::fs::File;
use std::process;

use libaeon::bytecode_parser;
use libaeon::virtual_machine::VirtualMachine;
use libaeon::virtual_machine_methods::VirtualMachineMethods;

fn print_usage(options: &getopts::Options) {
    println!("{}", options.usage("Usage: aeon FILE [OPTIONS]"));
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut options       = getopts::Options::new();

    options.optflag("h", "help", "Shows this help message");
    options.optflag("v", "version", "Prints the version number");

    let matches = match options.parse(&args[1..]) {
        Ok(matches) => matches,
        Err(error)  => panic!(error.to_string())
    };

    if matches.opt_present("h") {
        print_usage(&options);
    }

    if matches.opt_present("v") {
        println!("aeon {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if matches.free.is_empty() {
        print_usage(&options);
    }
    else {
        let ref path = matches.free[0];

        match File::open(path) {
            Ok(file) => {
                let mut bytes = file.bytes();

                match bytecode_parser::parse(&mut bytes) {
                    Ok(code) => {
                        let vm     = VirtualMachine::new();
                        let status = vm.start(code);

                        if status.is_err() {
                            process::exit(1);
                        }
                    },
                    Err(error) => {
                        println!("Failed to parse file {}: {:?}", path, error);
                        process::exit(1);
                    }
                }
            },
            Err(error) => {
                println!("Failed to execute {}: {}", path, error.to_string());
                process::exit(1);
            }
        }
    }
}
