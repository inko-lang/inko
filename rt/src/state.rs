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

pub(crate) struct Env {
    pub(crate) keys: Vec<*const InkoString>,
    pub(crate) mapping: HashMap<String, *const InkoString>,
}

impl Env {
    fn new(class: ClassPointer) -> Env {
        let mut keys = Vec::new();
        let mut mapping = HashMap::new();

        for (k, v) in env::vars_os() {
            let raw_key = k.to_string_lossy().into_owned();
            let key = InkoString::alloc_permanent(class, raw_key.clone());
            let val = InkoString::alloc_permanent(
                class,
                v.to_string_lossy().into_owned(),
            );

            keys.push(key);
            mapping.insert(raw_key, val);
        }

        Env { keys, mapping }
    }

    pub(crate) fn get(&self, key: &str) -> Option<*const InkoString> {
        self.mapping.get(key).cloned()
    }

    pub(crate) fn key(&self, index: usize) -> Option<*const InkoString> {
        self.keys.get(index).cloned()
    }

    pub(crate) fn len(&self) -> usize {
        self.mapping.len()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        unsafe {
            for &val in self.mapping.values() {
                InkoString::drop_and_deallocate(val);
            }

            for &val in &self.keys {
                InkoString::drop_and_deallocate(val);
            }
        }
    }
}

/// The state of the Inko runtime.
#[repr(C)]
pub struct State {
    // These fields are exposed to the generated code directly, hence they're
    // marked as public. These fields must also come first so their offsets are
    // more reliable/stable.
    //
    // I repeat, _do not reorder_ these public fields without also updating both
    // the compiler and standard library in the necessary places.
    pub true_singleton: *const Bool,
    pub false_singleton: *const Bool,
    pub nil_singleton: *const Nil,
    pub int_class: ClassPointer,
    pub float_class: ClassPointer,
    pub string_class: ClassPointer,
    pub array_class: ClassPointer,
    pub bool_class: ClassPointer,
    pub nil_class: ClassPointer,
    pub byte_array_class: ClassPointer,
    pub channel_class: ClassPointer,

    /// The first randomly generated key to use for hashers.
    pub hash_key0: *const Int,

    /// The second randomly generated key to use for hashers.
    pub hash_key1: *const Int,

    /// The runtime's configuration.
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
        args: Vec<String>,
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
            .into_iter()
            .map(|arg| InkoString::alloc_permanent(string_class, arg))
            .collect();

        let mut rng = thread_rng();
        let hash_key0 = Int::new_permanent(int_class, rng.gen());
        let hash_key1 = Int::new_permanent(int_class, rng.gen());
        let environment = Env::new(string_class);
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

        assert_eq!(offset_of!(state, true_singleton), 0);
        assert_eq!(offset_of!(state, false_singleton), 8);
        assert_eq!(offset_of!(state, nil_singleton), 16);

        // These offsets are tested against because the runtime makes use of
        // them.
        assert_eq!(offset_of!(state, hash_key0), 88);
        assert_eq!(offset_of!(state, hash_key1), 96);
    }
}
