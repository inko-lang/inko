extern crate getopts;
extern crate libinko;

use getopts::{Options, ParsingStyle};
use std::env;
use std::io::{self, Write};
use std::process;

use libinko::config::Config;
use libinko::vm::machine::Machine;
use libinko::vm::state::State;

fn print_usage(options: &Options) {
    print_stderr(&options.usage("Usage: ivm FILE [OPTIONS]"));
}

fn print_stderr(message: &str) {
    let mut stderr = io::stderr();

    stderr.write_all(message.as_bytes()).unwrap();
    stderr.write_all(b"\n").unwrap();
    stderr.flush().unwrap();
}

fn run() -> i32 {
    let args: Vec<String> = env::args().collect();
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("h", "help", "Shows this help message");
    options.optflag("v", "version", "Prints the version number");
    options.optflag("f", "features", "Prints various enabled VM features");

    let matches = match options.parse(&args[1..]) {
        Ok(matches) => matches,
        Err(err) => {
            print_stderr(&format!("{}\n", err));
            print_usage(&options);
            return 1;
        }
    };

    if matches.opt_present("h") {
        print_usage(&options);
        return 0;
    }

    if matches.opt_present("v") {
        println!("ivm {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }

    if matches.opt_present("f") {
        println!("AES-NI: {}", cfg!(target_feature = "aes"));
        println!("jemalloc: {}", cfg!(feature = "jemalloc"));
        return 0;
    }

    if matches.free.is_empty() {
        print_usage(&options);

        1
    } else {
        let mut config = Config::new();
        let path = &matches.free[0];

        config.populate_from_env();

        let machine = Machine::new(State::with_rc(config, &matches.free[1..]));

        machine.start(path);
        machine.state.current_exit_status()
    }
}

fn main() {
    process::exit(run());
}
