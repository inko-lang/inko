extern crate getopts;
extern crate libinko;

use std::io::{self, Write};
use std::env;
use std::process;

use libinko::config::Config;
use libinko::vm::machine::Machine;
use libinko::vm::state::State;

fn print_usage(options: &getopts::Options) -> ! {
    print_stderr(format!("{}", options.usage("Usage: ivm FILE [OPTIONS]")));

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
        "A directory to search for bytecode files",
        "DIR",
    );

    let matches = options.parse(&args[1..]).unwrap_or_else(|err| {
        print_stderr(format!("{}\n", err));
        print_usage(&options);
    });

    if matches.opt_present("h") {
        print_usage(&options);
    }

    if matches.opt_present("v") {
        println!("ivm {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if matches.free.is_empty() {
        print_usage(&options);
    } else {
        let mut config = Config::new();
        let ref path = matches.free[0];

        if matches.opt_present("I") {
            for dir in matches.opt_strs("I") {
                config.add_directory(dir);
            }
        }

        config.populate_from_env();

        let machine = Machine::default(State::new(config));

        if !machine.start(path) {
            process::exit(1);
        }
    }
}
