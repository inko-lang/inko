//! Virtual Machine Configuration
//!
//! Various virtual machine settings can be changed by the user, such as the
//! directories to search for bytecode files and the number of operating system
//! threads to use for running VM processes.
//!
//! These settings are all stored in the Config struct, allowing various parts
//! of the VM to easily access these configuration details.
use crate::immix::block::BLOCK_SIZE;
use num_cpus;
use std::env;
use std::path::PathBuf;

/// Sets a configuration field based on an environment variable.
macro_rules! set_from_env {
    ($config:expr, $field:ident, $key:expr, $value_type:ty) => {{
        if let Ok(raw_value) = env::var(concat!("INKO_", $key)) {
            if let Ok(value) = raw_value.parse::<$value_type>() {
                $config.$field = value;
            }
        };
    }};
}

const DEFAULT_YOUNG_THRESHOLD: u32 = (8 * 1024 * 1024) / (BLOCK_SIZE as u32);
const DEFAULT_MATURE_THRESHOLD: u32 = (16 * 1024 * 1024) / (BLOCK_SIZE as u32);
const DEFAULT_MAILBOX_THRESHOLD: u32 = 1;
const DEFAULT_GROWTH_FACTOR: f64 = 1.5;
const DEFAULT_GROWTH_THRESHOLD: f64 = 0.9;
const DEFAULT_REDUCTIONS: usize = 1000;

/// Structure containing the configuration settings for the virtual machine.
pub struct Config {
    /// The directories to search in for extra bytecode files to run.
    pub directories: Vec<PathBuf>,

    /// The number of primary process threads to run.
    pub primary_threads: usize,

    /// The number of blocking process threads to run.
    pub blocking_threads: usize,

    /// The number of garbage collector threads to run.
    pub gc_threads: usize,

    /// The number of finalizer threads to run.
    pub finalizer_threads: usize,

    /// The number of threads to use for various generic parallel tasks such as
    /// scanning stack frames during garbage collection.
    pub generic_parallel_threads: usize,

    /// The number of reductions a process can perform before being suspended.
    /// Defaults to 1000.
    pub reductions: usize,

    /// The number of memory blocks that can be allocated before triggering a
    /// young collection.
    pub young_threshold: u32,

    /// The number of memory blocks that can be allocated before triggering a
    /// mature collection.
    pub mature_threshold: u32,

    /// The block allocation growth factor for the heap.
    pub heap_growth_factor: f64,

    /// The percentage of memory in the heap (relative to its threshold) that
    /// should be used before increasing the heap size.
    pub heap_growth_threshold: f64,

    /// The number of memory blocks that can be allocated before triggering a
    /// mailbox collection.
    pub mailbox_threshold: u32,

    /// The block allocation growth factor for the mailbox heap.
    pub mailbox_growth_factor: f64,

    /// The percentage of memory in the mailbox heap that should be used before
    /// increasing the size.
    pub mailbox_growth_threshold: f64,
}

impl Config {
    pub fn new() -> Config {
        let cpu_count = num_cpus::get();

        Config {
            directories: Vec::new(),
            primary_threads: cpu_count,
            gc_threads: cpu_count,
            finalizer_threads: cpu_count,
            blocking_threads: cpu_count,
            generic_parallel_threads: cpu_count,
            reductions: DEFAULT_REDUCTIONS,
            young_threshold: DEFAULT_YOUNG_THRESHOLD,
            mature_threshold: DEFAULT_MATURE_THRESHOLD,
            heap_growth_factor: DEFAULT_GROWTH_FACTOR,
            heap_growth_threshold: DEFAULT_GROWTH_THRESHOLD,
            mailbox_threshold: DEFAULT_MAILBOX_THRESHOLD,
            mailbox_growth_factor: DEFAULT_GROWTH_FACTOR,
            mailbox_growth_threshold: DEFAULT_GROWTH_THRESHOLD,
        }
    }

    /// Populates configuration settings based on environment variables.
    #[cfg_attr(feature = "cargo-clippy", allow(cyclomatic_complexity))]
    pub fn populate_from_env(&mut self) {
        set_from_env!(self, primary_threads, "CONCURRENCY", usize);
        set_from_env!(self, blocking_threads, "CONCURRENCY", usize);
        set_from_env!(self, gc_threads, "CONCURRENCY", usize);
        set_from_env!(self, finalizer_threads, "CONCURRENCY", usize);
        set_from_env!(self, generic_parallel_threads, "CONCURRENCY", usize);

        set_from_env!(self, reductions, "REDUCTIONS", usize);

        set_from_env!(self, young_threshold, "YOUNG_THRESHOLD", u32);
        set_from_env!(self, mature_threshold, "MATURE_THRESHOLD", u32);
        set_from_env!(self, heap_growth_factor, "HEAP_GROWTH_FACTOR", f64);

        set_from_env!(
            self,
            heap_growth_threshold,
            "HEAP_GROWTH_THRESHOLD",
            f64
        );

        set_from_env!(self, mailbox_threshold, "MAILBOX_THRESHOLD", u32);

        set_from_env!(
            self,
            mailbox_growth_factor,
            "MAILBOX_GROWTH_FACTOR",
            f64
        );

        set_from_env!(
            self,
            mailbox_growth_threshold,
            "MAILBOX_GROWTH_THRESHOLD",
            f64
        );
    }

    pub fn add_directory(&mut self, path: String) {
        self.directories.push(PathBuf::from(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_new() {
        let config = Config::new();

        assert_eq!(config.directories.len(), 0);
        assert!(config.primary_threads >= 1);
        assert!(config.gc_threads >= 1);
        assert_eq!(config.reductions, 1000);
    }

    #[test]
    fn test_populate_from_env() {
        env::set_var("INKO_CONCURRENCY", "42");
        env::set_var("INKO_HEAP_GROWTH_FACTOR", "4.2");

        let mut config = Config::new();

        config.populate_from_env();

        // Unset before any assertions may fail.
        env::remove_var("INKO_CONCURRENCY");
        env::remove_var("INKO_HEAP_GROWTH_FACTOR");

        assert_eq!(config.primary_threads, 42);
        assert_eq!(config.heap_growth_factor, 4.2);
    }

    #[test]
    fn test_add_directory() {
        let mut config = Config::new();

        config.add_directory("foo".to_string());

        assert_eq!(config.directories.len(), 1);
    }
}
