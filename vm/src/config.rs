//! Virtual Machine Configuration
//!
//! Various virtual machine settings that can be changed by the user, such as
//! the number of threads to run.
use std::env::var;

/// Sets a configuration field based on an environment variable.
macro_rules! set_from_env {
    ($config:expr, $field:ident, $key:expr, $value_type:ty) => {{
        if let Ok(raw_value) = var(concat!("INKO_", $key)) {
            if let Ok(value) = raw_value.parse::<$value_type>() {
                $config.$field = value;
            }
        };
    }};
}

const DEFAULT_REDUCTIONS: u16 = 1000;

/// Structure containing the configuration settings for the virtual machine.
pub struct Config {
    /// The number of process threads to run.
    pub process_threads: u16,

    /// The number of reductions a process can perform before being suspended.
    pub reductions: u16,
}

impl Config {
    pub fn new() -> Config {
        let cpu_count = num_cpus::get();

        Config {
            process_threads: cpu_count as u16,
            reductions: DEFAULT_REDUCTIONS,
        }
    }

    pub fn from_env() -> Config {
        let mut config = Config::new();

        set_from_env!(config, process_threads, "PROCESS_THREADS", u16);
        set_from_env!(config, reductions, "REDUCTIONS", u16);

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(key: &str) -> Result<&str, ()> {
        match key {
            "INKO_FOO" => Ok("1"),
            "INKO_BAR" => Ok("0"),
            _ => Err(()),
        }
    }

    #[test]
    fn test_new() {
        let config = Config::new();

        assert!(config.process_threads >= 1);
        assert_eq!(config.reductions, DEFAULT_REDUCTIONS);
    }

    #[test]
    fn test_set_from_env() {
        let mut cfg = Config::new();

        set_from_env!(cfg, process_threads, "FOO", u16);
        set_from_env!(cfg, reductions, "BAR", u16);

        assert_eq!(cfg.process_threads, 1);
        assert_eq!(cfg.reductions, 0);

        set_from_env!(cfg, reductions, "BAZ", u16);

        assert_eq!(cfg.reductions, 0);
    }
}
