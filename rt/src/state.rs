use crate::arc_without_weak::ArcWithoutWeak;
use crate::config::Config;
use crate::mem::{ByteArray, Class, ClassPointer, String as InkoString};
use crate::network_poller::NetworkPoller;
use crate::scheduler::process::Scheduler;
use crate::scheduler::timeout_worker::TimeoutWorker;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::env;
use std::mem::size_of;
use std::panic::RefUnwindSafe;
use std::time;

/// Allocates a new class, returning a tuple containing the owned pointer and a
/// permanent reference pointer.
macro_rules! class {
    ($name: expr, $methods: expr, $size_source: ident) => {{
        Class::alloc(
            $name.to_string(),
            $methods,
            size_of::<$size_source>() as u32,
        )
    }};
}

/// A reference counted State.
pub(crate) type RcState = ArcWithoutWeak<State>;

/// The number of methods used for the various built-in classes.
///
/// These counts are used to determine how much memory is needed for allocating
/// the various built-in classes.
#[derive(Default, Debug)]
#[repr(C)]
pub struct MethodCounts {
    pub(crate) string_class: u16,
    pub(crate) byte_array_class: u16,
}

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
    pub string_class: ClassPointer,
    pub byte_array_class: ClassPointer,

    /// The first randomly generated key to use for hashers.
    pub hash_key0: i64,

    /// The second randomly generated key to use for hashers.
    pub hash_key1: i64,

    /// The runtime's configuration.
    pub(crate) config: Config,

    /// The start time of the VM (more or less).
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
}

unsafe impl Sync for State {}
impl RefUnwindSafe for State {}

impl State {
    pub(crate) fn new(
        config: Config,
        counts: &MethodCounts,
        arguments: Vec<String>,
    ) -> RcState {
        let string_class = class!("String", counts.string_class, InkoString);
        let byte_array_class =
            class!("ByteArray", counts.byte_array_class, ByteArray);

        let mut rng = thread_rng();
        let hash_key0 = rng.gen();
        let hash_key1 = rng.gen();
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
            scheduler,
            environment,
            config,
            start_time: time::Instant::now(),
            timeout_worker: TimeoutWorker::new(),
            arguments,
            network_pollers,
            string_class,
            byte_array_class,
        };

        ArcWithoutWeak::new(state)
    }

    pub(crate) fn terminate(&self) {
        self.scheduler.terminate();
    }
}

impl Drop for State {
    fn drop(&mut self) {
        unsafe {
            Class::drop(self.string_class);
            Class::drop(self.byte_array_class);
        }
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
        let state = State::new(config, &MethodCounts::default(), Vec::new());

        // These offsets are tested against because the runtime makes use of
        // them.
        assert_eq!(offset_of!(state, hash_key0), 16);
        assert_eq!(offset_of!(state, hash_key1), 24);
    }
}
