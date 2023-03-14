use crate::arc_without_weak::ArcWithoutWeak;
use crate::config::Config;
use crate::mem::{
    Array, Bool, ByteArray, Class, ClassPointer, Float, Int, Nil,
    String as InkoString,
};
use crate::network_poller::NetworkPoller;
use crate::process::Channel;
use crate::scheduler::process::Scheduler;
use crate::scheduler::timeout_worker::TimeoutWorker;
use ahash::RandomState;
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
    pub(crate) int_class: u16,
    pub(crate) float_class: u16,
    pub(crate) string_class: u16,
    pub(crate) array_class: u16,
    pub(crate) boolean_class: u16,
    pub(crate) nil_class: u16,
    pub(crate) byte_array_class: u16,
    pub(crate) channel_class: u16,
}

/// The state of a virtual machine.
#[repr(C)]
pub struct State {
    // These fields are exposed to the generated code directly, hence they're
    // marked as public. These fields must also come first so their offsets are
    // more reliable/stable.
    pub true_singleton: *const Bool,
    pub false_singleton: *const Bool,
    pub nil_singleton: *const Nil,

    pub(crate) int_class: ClassPointer,
    pub(crate) float_class: ClassPointer,
    pub(crate) string_class: ClassPointer,
    pub(crate) array_class: ClassPointer,
    pub(crate) bool_class: ClassPointer,
    pub(crate) nil_class: ClassPointer,
    pub(crate) byte_array_class: ClassPointer,
    pub(crate) channel_class: ClassPointer,

    /// The virtual machine's configuration.
    pub(crate) config: Config,

    /// The start time of the VM (more or less).
    pub(crate) start_time: time::Instant,

    /// The commandline arguments passed to an Inko program.
    pub(crate) arguments: Vec<*const InkoString>,

    /// The environment variables defined when the VM started.
    ///
    /// We cache environment variables because C functions used through the FFI
    /// (or through libraries) may call `setenv()` concurrently with `getenv()`
    /// calls, which is unsound. Caching the variables also means we can safely
    /// use `localtime_r()` (which internally may call `setenv()`).
    pub(crate) environment: HashMap<String, *const InkoString>,

    /// The scheduler to use for executing Inko processes.
    pub(crate) scheduler: Scheduler,

    /// A task used for handling timeouts, such as message and IO timeouts.
    pub(crate) timeout_worker: TimeoutWorker,

    /// The network pollers to use for process threads.
    pub(crate) network_pollers: Vec<NetworkPoller>,

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

unsafe impl Sync for State {}
impl RefUnwindSafe for State {}

impl State {
    pub(crate) fn new(
        config: Config,
        counts: &MethodCounts,
        args: &[String],
    ) -> RcState {
        let int_class = class!("Int", counts.int_class, Int);
        let float_class = class!("Float", counts.float_class, Float);
        let string_class = class!("String", counts.string_class, InkoString);
        let array_class = class!("Array", counts.array_class, Array);
        let bool_class = class!("Bool", counts.boolean_class, Bool);
        let nil_class = class!("Nil", counts.nil_class, Nil);
        let byte_array_class =
            class!("ByteArray", counts.byte_array_class, ByteArray);
        let channel_class = class!("Channel", counts.channel_class, Channel);

        let true_singleton = Bool::alloc(bool_class);
        let false_singleton = Bool::alloc(bool_class);
        let nil_singleton = Nil::alloc(nil_class);

        let arguments = args
            .iter()
            .map(|arg| InkoString::alloc_permanent(string_class, arg.clone()))
            .collect();

        let mut rng = thread_rng();
        let hash_state =
            RandomState::with_seeds(rng.gen(), rng.gen(), rng.gen(), rng.gen());

        let environment = env::vars_os()
            .into_iter()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    InkoString::alloc_permanent(
                        string_class,
                        v.to_string_lossy().into_owned(),
                    ),
                )
            })
            .collect::<HashMap<_, _>>();

        let scheduler = Scheduler::new(
            config.process_threads as usize,
            config.backup_threads as usize,
            config.stack_size as usize,
        );

        let network_pollers =
            (0..config.netpoll_threads).map(|_| NetworkPoller::new()).collect();

        let state = State {
            scheduler,
            environment,
            config,
            start_time: time::Instant::now(),
            timeout_worker: TimeoutWorker::new(),
            arguments,
            network_pollers,
            hash_state,
            int_class,
            float_class,
            string_class,
            array_class,
            bool_class,
            nil_class,
            byte_array_class,
            channel_class,
            true_singleton,
            false_singleton,
            nil_singleton,
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
            for &val in &self.arguments {
                InkoString::drop_and_deallocate(val)
            }

            for &val in self.environment.values() {
                InkoString::drop_and_deallocate(val);
            }

            Bool::drop_and_deallocate(self.true_singleton);
            Bool::drop_and_deallocate(self.false_singleton);
            Nil::drop_and_deallocate(self.nil_singleton);

            Class::drop(self.int_class);
            Class::drop(self.float_class);
            Class::drop(self.string_class);
            Class::drop(self.array_class);
            Class::drop(self.bool_class);
            Class::drop(self.nil_class);
            Class::drop(self.byte_array_class);
            Class::drop(self.channel_class);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    macro_rules! offset_of {
        ($value: expr, $field: ident) => {{
            (std::ptr::addr_of!($value.$field) as usize)
                - (&*$value as *const _ as usize)
        }};
    }

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<State>(), 384);
    }

    #[test]
    fn test_field_offsets() {
        let config = Config::new();
        let state = State::new(config, &MethodCounts::default(), &[]);

        assert_eq!(offset_of!(state, true_singleton), 0);
        assert_eq!(offset_of!(state, false_singleton), 8);
        assert_eq!(offset_of!(state, nil_singleton), 16);
    }
}
