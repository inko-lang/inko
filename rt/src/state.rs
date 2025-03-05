use crate::arc_without_weak::ArcWithoutWeak;
use crate::config::Config;
use crate::network_poller::NetworkPoller;
use crate::scheduler::process::Scheduler;
use crate::scheduler::signal::Signals;
use crate::scheduler::timeouts::Worker as TimeoutWorker;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::env;
use std::hash::{BuildHasher, Hasher};
use std::panic::RefUnwindSafe;
use std::sync::atomic::AtomicU32;
use std::thread::available_parallelism;
use std::time;

/// A reference counted State.
pub(crate) type RcState = ArcWithoutWeak<State>;

pub(crate) struct Env {
    pub(crate) keys: Vec<String>,
    pub(crate) mapping: HashMap<String, String>,
}

impl Env {
    fn new() -> Env {
        let mut keys = Vec::new();
        let mut mapping = HashMap::new();

        for (k, v) in env::vars_os() {
            let key = k.to_string_lossy().into_owned();
            let val = v.to_string_lossy().into_owned();

            keys.push(key.clone());
            mapping.insert(key, val);
        }

        Env { keys, mapping }
    }

    pub(crate) fn get(&self, key: &str) -> Option<&String> {
        self.mapping.get(key)
    }

    pub(crate) fn key(&self, index: usize) -> Option<&String> {
        self.keys.get(index)
    }

    pub(crate) fn len(&self) -> usize {
        self.mapping.len()
    }
}

/// The state of the Inko runtime.
#[repr(C)]
pub struct State {
    /// The first randomly generated key to use for hashers.
    pub hash_key0: i64,

    /// The second randomly generated key to use for hashers.
    pub hash_key1: i64,

    /// The scheduler epoch.
    ///
    /// When starting/resuming a process, this value is read into a
    /// process-local field. The process periodically compares its local value
    /// with this global value, and yields back to the scheduler if the
    /// difference is too great.
    ///
    /// This field is stored here and not in the `Scheduler` so it's easier to
    /// access from the generated code.
    pub scheduler_epoch: AtomicU32,

    /// The number of logical cores available to the current program.
    ///
    /// We retrieve this value once and store it, as `available_parallelism()`
    /// is affected by `sched_setaffinity(2)`, which is used for e.g. pinning
    /// worker threads to cores.
    pub cores: i64,

    /// The runtime's configuration.
    pub(crate) config: Config,

    /// The start time of the program (more or less).
    pub(crate) start_time: time::Instant,

    /// The commandline arguments passed to an Inko program.
    pub(crate) arguments: Vec<String>,

    /// The environment variables defined when the VM started.
    ///
    /// We cache environment variables because C functions used through the FFI
    /// (or through libraries) may call `setenv()` concurrently with `getenv()`
    /// calls, which is unsound. Caching the variables also means we can safely
    /// use `localtime_r()` (which internally may call `setenv()`).
    pub(crate) environment: Env,

    /// The scheduler to use for executing Inko processes.
    pub(crate) scheduler: Scheduler,

    /// A task used for handling timeouts, such as message and IO timeouts.
    pub(crate) timeout_worker: TimeoutWorker,

    /// The network pollers to use for process threads.
    pub(crate) network_pollers: Vec<NetworkPoller>,

    pub(crate) signals: Signals,
}

unsafe impl Sync for State {}
impl RefUnwindSafe for State {}

impl State {
    pub(crate) fn new(config: Config, arguments: Vec<String>) -> RcState {
        let hash_key0 = RandomState::new().build_hasher().finish() as i64;
        let hash_key1 = RandomState::new().build_hasher().finish() as i64;
        let environment = Env::new();
        let scheduler = Scheduler::new(
            config.process_threads as usize,
            config.backup_threads as usize,
            config.stack_size as usize,
        );

        let network_pollers =
            (0..config.netpoll_threads).map(|_| NetworkPoller::new()).collect();

        let state = State {
            hash_key0,
            hash_key1,
            scheduler_epoch: AtomicU32::new(0),
            cores: available_parallelism().map(|v| v.get()).unwrap_or(1) as i64,
            scheduler,
            environment,
            config,
            start_time: time::Instant::now(),
            timeout_worker: TimeoutWorker::new(),
            arguments,
            network_pollers,
            signals: Signals::new(),
        };

        ArcWithoutWeak::new(state)
    }

    pub(crate) fn terminate(&self) {
        self.scheduler.terminate();
        self.timeout_worker.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! offset_of {
        ($value: expr, $field: ident) => {{
            (std::ptr::addr_of!($value.$field) as usize)
                - (&*$value as *const _ as usize)
        }};
    }

    #[test]
    fn test_field_offsets() {
        let config = Config::new();
        let state = State::new(config, Vec::new());

        // These offsets are tested against because the runtime makes use of
        // them.
        assert_eq!(offset_of!(state, hash_key0), 0);
        assert_eq!(offset_of!(state, hash_key1), 8);
        assert_eq!(offset_of!(state, cores), 24);
    }
}
