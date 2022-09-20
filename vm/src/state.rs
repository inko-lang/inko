use crate::arc_without_weak::ArcWithoutWeak;
use crate::builtin_functions::BuiltinFunctions;
use crate::config::Config;
use crate::mem::Pointer;
use crate::network_poller::NetworkPoller;
use crate::permanent_space::PermanentSpace;
use crate::scheduler::process::Scheduler;
use crate::scheduler::timeout_worker::TimeoutWorker;
use ahash::RandomState;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::env;
use std::panic::RefUnwindSafe;
use std::sync::Mutex;
use std::time;

/// A reference counted State.
pub(crate) type RcState = ArcWithoutWeak<State>;

/// The state of a virtual machine.
pub(crate) struct State {
    /// The virtual machine's configuration.
    pub config: Config,

    /// The start time of the VM (more or less).
    pub start_time: time::Instant,

    /// The commandline arguments passed to an Inko program.
    pub arguments: Vec<Pointer>,

    /// The environment variables defined when the VM started.
    ///
    /// We cache environment variables because C functions used through the FFI
    /// (or through libraries) may call `setenv()` concurrently with `getenv()`
    /// calls, which is unsound. Caching the variables also means we can safely
    /// use `localtime_r()` (which internally may call `setenv()`).
    pub environment: HashMap<String, Pointer>,

    /// The exit status to use when the VM terminates.
    pub exit_status: Mutex<i32>,

    /// The scheduler to use for executing Inko processes.
    pub scheduler: Scheduler,

    /// A task used for handling timeouts, such as message and IO timeouts.
    pub timeout_worker: TimeoutWorker,

    /// The system polling mechanism to use for polling non-blocking sockets.
    pub network_poller: NetworkPoller,

    /// All builtin functions that a compiler can use.
    pub builtin_functions: BuiltinFunctions,

    /// A type for allocating and storing blocks and permanent objects.
    pub permanent_space: PermanentSpace,

    /// The random state to use for building hashers.
    ///
    /// We use the same base state for all hashers, seeded with randomly
    /// generated keys. This means all hashers start off with the same base
    /// state, and thus produce the same hash codes.
    ///
    /// The alternative is to generate unique seeds for every hasher, in a way
    /// that Rust does (= starting off with a thread-local randomly generated
    /// number, then incrementing it). This however requires that we somehow
    /// expose the means to generate such keys to the standard library, such
    /// that it can reuse these where necessary (e.g. a `Map` needs to produce
    /// the same results for the same values every time). This leads to
    /// implementation details leaking into the standard library, and we want to
    /// avoid that.
    pub(crate) hash_state: RandomState,
}

impl RefUnwindSafe for State {}

impl State {
    pub(crate) fn new(
        config: Config,
        permanent_space: PermanentSpace,
        args: &[String],
    ) -> RcState {
        let arguments = args
            .iter()
            .map(|arg| permanent_space.allocate_string(arg.clone()))
            .collect();

        let mut rng = thread_rng();
        let hash_state =
            RandomState::with_seeds(rng.gen(), rng.gen(), rng.gen(), rng.gen());

        let environment = env::vars_os()
            .into_iter()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    permanent_space
                        .allocate_string(v.to_string_lossy().into_owned()),
                )
            })
            .collect::<HashMap<_, _>>();

        let scheduler = Scheduler::new(
            config.process_threads as usize,
            config.backup_threads as usize,
        );

        let state = State {
            scheduler,
            environment,
            config,
            start_time: time::Instant::now(),
            exit_status: Mutex::new(0),
            timeout_worker: TimeoutWorker::new(),
            arguments,
            network_poller: NetworkPoller::new(),
            builtin_functions: BuiltinFunctions::new(),
            permanent_space,
            hash_state,
        };

        ArcWithoutWeak::new(state)
    }

    pub(crate) fn terminate(&self) {
        self.scheduler.terminate();
    }

    pub(crate) fn set_exit_status(&self, new_status: i32) {
        *self.exit_status.lock().unwrap() = new_status;
    }

    pub(crate) fn current_exit_status(&self) -> i32 {
        *self.exit_status.lock().unwrap()
    }
}
