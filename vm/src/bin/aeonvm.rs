extern crate libaeon;
extern crate getopts;

use std::io::prelude::*;
use std::env;
use std::fs::File;
use std::process;
use std::mem;

use libaeon::bytecode_parser;
use libaeon::config::Config;
use libaeon::virtual_machine::{VirtualMachine, VirtualMachineState};

fn print_usage(options: &getopts::Options) -> ! {
    println!("{}", options.usage("Usage: aeonvm FILE [OPTIONS]"));
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut options = getopts::Options::new();

    options.optflag("h", "help", "Shows this help message");
    options.optflag("v", "version", "Prints the version number");

    options.optmulti("I",
                     "include",
                     "A directory to search for bytecode files",
                     "DIR");

    options.optopt("",
                   "pthreads",
                   "The number of threads to use for running processes",
                   "INT");

    options.optopt("", "gcthreads", "The number of GC threads to use", "INT");

    options.optopt("",
                   "reductions",
                   "The number of reductions that can take place",
                   "INT");

    options.optflag("", "type-sizes", "Prints the size of various VM types");

    let matches = match options.parse(&args[1..]) {
        Ok(matches) => matches,
        Err(error) => {
            println!("{}", error.to_string());
            print_usage(&options);
        }
    };

    if matches.opt_present("h") {
        print_usage(&options);
    }

    if matches.opt_present("v") {
        println!("aeonvm {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if matches.opt_present("type-sizes") {
        println!("CallFrame: {} bytes",
                 mem::size_of::<libaeon::call_frame::CallFrame>());

        println!("Heap: {} bytes", mem::size_of::<libaeon::heap::Heap>());

        println!("Mailbox: {} bytes",
                 mem::size_of::<libaeon::mailbox::Mailbox>());

        println!("Process: {} bytes",
                 mem::size_of::<libaeon::process::Process>());

        println!("ExecutionContext: {} bytes",
                 mem::size_of::<libaeon::execution_context::ExecutionContext>());

        println!("Thread: {} bytes",
                 mem::size_of::<libaeon::thread::Thread>());

        println!("Register: {} bytes",
                 mem::size_of::<libaeon::register::Register>());

        println!("Object: {} bytes",
                 mem::size_of::<libaeon::object::Object>());

        println!("ObjectValue: {} bytes",
                 mem::size_of::<libaeon::object_value::ObjectValue>());

        println!("ObjectPointer: {} bytes",
                 mem::size_of::<libaeon::object_pointer::ObjectPointer>());

        println!("ObjectHeader: {} bytes",
                 mem::size_of::<libaeon::object_header::ObjectHeader>());

        return;
    }

    if matches.free.is_empty() {
        print_usage(&options);
    } else {
        let mut config = Config::new();
        let ref path = matches.free[0];

        if let Some(pthreads) = matches.opt_str("pthreads") {
            config.set_process_threads(pthreads.parse::<usize>().unwrap());
        }

        if let Some(gc_threads) = matches.opt_str("gcthreads") {
            config.set_gc_threads(gc_threads.parse::<usize>().unwrap());
        }

        if let Some(reductions) = matches.opt_str("reductions") {
            config.set_reductions(reductions.parse::<usize>().unwrap());
        }

        if matches.opt_present("I") {
            for dir in matches.opt_strs("I") {
                config.add_directory(dir);
            }
        }

        match File::open(path) {
            Ok(file) => {
                let mut bytes = file.bytes();

                match bytecode_parser::parse(&mut bytes) {
                    Ok(code) => {
                        let state = VirtualMachineState::new(config);
                        let vm = VirtualMachine::new(state);
                        let status = vm.start(code);

                        if status.is_err() {
                            process::exit(1);
                        }
                    }
                    Err(error) => {
                        println!("Failed to parse file {}: {:?}", path, error);
                        process::exit(1);
                    }
                }
            }
            Err(error) => {
                println!("Failed to execute {}: {}", path, error.to_string());
                process::exit(1);
            }
        }
    }
}
