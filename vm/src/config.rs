extern crate num_cpus;

use std::path::PathBuf;

pub struct Config {
    pub directories: Vec<PathBuf>,
    pub process_threads: usize,
}

impl Config {
    pub fn new() -> Config {
        Config {
            directories: Vec::new(),
            process_threads: num_cpus::get(),
        }
    }

    pub fn add_directory(&mut self, path: String) {
        self.directories.push(PathBuf::from(path));
    }

    pub fn set_process_threads(&mut self, threads: usize) {
        if threads == 0 {
            self.process_threads = 1;
        } else {
            self.process_threads = threads;
        }
    }
}
