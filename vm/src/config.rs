use num_cpus;
use std::path::PathBuf;

pub struct Config {
    pub directories: Vec<PathBuf>,
    pub process_threads: usize,
    pub gc_threads: usize,
    pub reductions: usize,
}

impl Config {
    pub fn new() -> Config {
        let cpu_count = num_cpus::get();

        Config {
            directories: Vec::new(),
            process_threads: cpu_count,
            gc_threads: cpu_count,
            reductions: 1000,
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

    pub fn set_gc_threads(&mut self, threads: usize) {
        if threads == 0 {
            self.gc_threads = 1;
        } else {
            self.gc_threads = threads;
        }
    }

    pub fn set_reductions(&mut self, reductions: usize) {
        if reductions > 0 {
            self.reductions = reductions;
        }
    }
}
