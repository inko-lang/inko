//! Virtual Machine Configuration
//!
//! Various virtual machine settings can be changed by the user, such as the
//! directories to search for bytecode files and the number of operating system
//! threads to use for running VM processes.
//!
//! These settings are all stored in the Config struct, allowing various parts
//! of the VM to easily access these configuration details.

use num_cpus;
use std::path::PathBuf;

/// Structure containing the configuration settings for the virtual machine.
pub struct Config {
    /// The directories to search in for extra bytecode files to run.
    pub directories: Vec<PathBuf>,

    /// The number of operating system processes to use for running virtual
    /// machine processes. Defaults to the number of CPU cores.
    pub process_threads: usize,

    /// The number of garbage collector threads to run. Defaults to the number
    /// of CPU cores.
    pub gc_threads: usize,

    /// The number of reductions a process can perform before being suspended.
    /// Defaults to 1000.
    pub reductions: usize,

    /// The block allocation growth factor for the young generation.
    pub young_growth_factor: f64,

    /// The block allocation growth factor for the mature generation.
    pub mature_growth_factor: f64,

    /// The block allocation growth factor for the mailbox space of every
    /// process..
    pub mailbox_growth_factor: f64,
}

impl Config {
    pub fn new() -> Config {
        let cpu_count = num_cpus::get();

        Config {
            directories: Vec::new(),
            process_threads: cpu_count,
            gc_threads: cpu_count,
            reductions: 1000,
            young_growth_factor: 1.5,
            mature_growth_factor: 1.5,
            mailbox_growth_factor: 1.5,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let config = Config::new();

        assert_eq!(config.directories.len(), 0);
        assert!(config.process_threads >= 1);
        assert!(config.gc_threads >= 1);
        assert_eq!(config.reductions, 1000);
    }

    #[test]
    fn test_add_directory() {
        let mut config = Config::new();

        config.add_directory("foo".to_string());

        assert_eq!(config.directories.len(), 1);
    }

    #[test]
    fn test_set_process_threads() {
        let mut config = Config::new();

        config.set_process_threads(5);

        assert_eq!(config.process_threads, 5);
    }

    #[test]
    fn test_set_gc_threads() {
        let mut config = Config::new();

        config.set_gc_threads(5);

        assert_eq!(config.gc_threads, 5);
    }

    #[test]
    fn test_set_reductions() {
        let mut config = Config::new();

        config.set_reductions(5);

        assert_eq!(config.reductions, 5);
    }
}
