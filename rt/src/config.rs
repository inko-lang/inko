use std::env::var;
use std::thread::available_parallelism;

/// Sets a configuration field based on an environment variable.
macro_rules! set_from_env {
    ($config:expr, $field:ident, $key:expr, $value_type:ty) => {{
        if let Ok(raw_value) = var(concat!("INKO_", $key)) {
            if let Ok(value) = raw_value.parse::<$value_type>() {
                if value > 0 {
                    $config.$field = value;
                }
            }
        };
    }};
}

/// The default number of network poller threads to use.
///
/// We default to one thread because for most setups this is probably more than
/// enough.
const DEFAULT_NETPOLL_THREADS: u8 = 1;

/// The maximum number of netpoll threads that are allowed.
const MAX_NETPOLL_THREADS: u8 = 127;

/// The default size of each process' stack in bytes.
///
/// The default size is chosen as we believe it to be large enough for most
/// cases, and to ensure foreign function calls don't overflow the stack.
///
/// This size is _excluding_ the padding introduced by guard pages, multiplying
/// by the page size, and rounding to the nearest power of two.
const DEFAULT_STACK_SIZE: u32 = 512 * 1024;

/// Structure containing the configuration settings for the virtual machine.
pub struct Config {
    /// The number of process threads to run.
    pub process_threads: u16,

    /// The number of backup process threads to spawn.
    pub backup_threads: u16,

    /// The size of each process' stack in bytes.
    pub stack_size: u32,

    /// The number of network poller threads to use.
    ///
    /// While this value is stored as an u8, it's limited to a maximum of 127.
    /// This is because internally we use an i8 to store registered poller IDs,
    /// and use the value -1 to signal a file descriptor isn't registered with
    /// any poller.
    pub netpoll_threads: u8,
}

impl Config {
    pub(crate) fn new() -> Config {
        let cpu_count =
            available_parallelism().map(|v| v.get()).unwrap_or(1) as u16;

        Config {
            process_threads: cpu_count,
            backup_threads: cpu_count * 4,
            netpoll_threads: DEFAULT_NETPOLL_THREADS,
            stack_size: DEFAULT_STACK_SIZE,
        }
    }

    pub(crate) fn from_env() -> Config {
        let mut config = Config::new();

        set_from_env!(config, process_threads, "PROCESS_THREADS", u16);
        set_from_env!(config, backup_threads, "BACKUP_THREADS", u16);
        set_from_env!(config, netpoll_threads, "NETPOLL_THREADS", u8);
        set_from_env!(config, stack_size, "STACK_SIZE", u32);

        config.verify();
        config
    }

    fn verify(&mut self) {
        if self.netpoll_threads > MAX_NETPOLL_THREADS {
            self.netpoll_threads = MAX_NETPOLL_THREADS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(key: &str) -> Result<&str, ()> {
        match key {
            "INKO_FOO" => Ok("1"),
            "INKO_BAR" => Ok("0"),
            "INKO_NETPOLL_THREADS" => Ok("4"),
            _ => Err(()),
        }
    }

    #[test]
    fn test_new() {
        let config = Config::new();

        assert!(config.process_threads >= 1);
    }

    #[test]
    fn test_set_from_env() {
        let mut cfg = Config::new();

        set_from_env!(cfg, process_threads, "FOO", u16);

        assert_eq!(cfg.process_threads, 1);
    }

    #[test]
    fn test_verify() {
        let mut cfg = Config::new();

        cfg.netpoll_threads = 64;
        cfg.verify();
        assert_eq!(cfg.netpoll_threads, 64);

        cfg.netpoll_threads = 130;
        cfg.verify();
        assert_eq!(cfg.netpoll_threads, MAX_NETPOLL_THREADS);
    }
}
