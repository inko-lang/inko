//! Virtual Machine Configuration
//!
//! Various virtual machine settings can be changed by the user, such as the
//! directories to search for bytecode files and the number of operating system
//! threads to use for running VM processes.
//!
//! These settings are all stored in the Config struct, allowing various parts
//! of the VM to easily access these configuration details.

use num_cpus;
use std::env;
use std::path::PathBuf;

/// Sets a configuration field based on an environment variable.
macro_rules! set_from_env {
    ($config: expr, $field: ident, $key: expr, $value_type: ty) => ({
        if let Ok(raw_value) = env::var(concat!("INKO_", $key)) {
            if let Ok(value) = raw_value.parse::<$value_type>() {
                $config.$field = value;
            }
        };
    });
}

/// Structure containing the configuration settings for the virtual machine.
pub struct Config {
    /// The directories to search in for extra bytecode files to run.
    pub directories: Vec<PathBuf>,

    /// The number of primary process threads to run.
    pub primary_threads: usize,

    /// The number of secondary process threads to run.
    pub secondary_threads: usize,

    /// The number of garbage collector threads to run. Defaults to half the
    /// number of CPU cores.
    pub gc_threads: usize,

    /// The number of reductions a process can perform before being suspended.
    /// Defaults to 1000.
    pub reductions: usize,

    /// The number of milliseconds to wait between checking for suspended
    /// processes.
    pub suspension_check_interval: u64,

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
            primary_threads: cpu_count,
            gc_threads: (cpu_count as f64 / 2.0_f64).ceil() as usize,
            secondary_threads: cpu_count,
            reductions: 1000,
            suspension_check_interval: 100,
            young_growth_factor: 1.5,
            mature_growth_factor: 1.5,
            mailbox_growth_factor: 1.5,
        }
    }

    /// Populates configuration settings based on environment variables.
    pub fn populate_from_env(&mut self) {
        set_from_env!(self, primary_threads, "PRIMARY_THREADS", usize);
        set_from_env!(self, secondary_threads, "SECONDARY_THREADS", usize);
        set_from_env!(self, gc_threads, "GC_THREADS", usize);

        set_from_env!(self, reductions, "REDUCTIONS", usize);
        set_from_env!(
            self,
            suspension_check_interval,
            "SUSPENSION_CHECK_INTERVAL",
            u64
        );

        set_from_env!(self, young_growth_factor, "GC_YOUNG_GROWTH_FACTOR", f64);
        set_from_env!(self, mature_growth_factor, "GC_MATURE_GROWTH_FACTOR", f64);

        set_from_env!(
            self,
            mailbox_growth_factor,
            "GC_MAILBOX_GROWTH_FACTOR",
            f64
        );
    }

    pub fn add_directory(&mut self, path: String) {
        self.directories.push(PathBuf::from(path));
    }

    pub fn set_primary_threads(&mut self, threads: usize) {
        if threads == 0 {
            self.primary_threads = 1;
        } else {
            self.primary_threads = threads;
        }
    }

    pub fn set_secondary_threads(&mut self, threads: usize) {
        if threads == 0 {
            self.secondary_threads = 1;
        } else {
            self.secondary_threads = threads;
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

    pub fn set_suspension_check_interval(&mut self, interval: u64) {
        if interval > 0 {
            self.suspension_check_interval = interval;
        }
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
        env::set_var("INKO_PRIMARY_THREADS", "42");
        env::set_var("INKO_GC_YOUNG_GROWTH_FACTOR", "4.2");

        let mut config = Config::new();

        config.populate_from_env();

        // Unset before any assertions may fail.
        env::remove_var("INKO_PROCESS_THREADS");
        env::remove_var("INKO_GC_YOUNG_GROWTH_FACTOR");

        assert_eq!(config.primary_threads, 42);
        assert_eq!(config.young_growth_factor, 4.2);
    }

    #[test]
    fn test_add_directory() {
        let mut config = Config::new();

        config.add_directory("foo".to_string());

        assert_eq!(config.directories.len(), 1);
    }

    #[test]
    fn test_set_primary_threads() {
        let mut config = Config::new();

        config.set_primary_threads(5);

        assert_eq!(config.primary_threads, 5);
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

    #[test]
    fn test_set_secondary_threads() {
        let mut config = Config::new();

        config.set_secondary_threads(2);

        assert_eq!(config.secondary_threads, 2);
    }
}
