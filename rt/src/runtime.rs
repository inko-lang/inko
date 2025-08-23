mod env;
mod float;
mod general;
mod int;
mod process;
mod signal;
mod socket;
mod string;
mod time;
mod tls;
mod types;

use crate::config::Config;
use crate::mem::TypePointer;
use crate::network_poller::Worker as NetworkPollerWorker;
use crate::process::{NativeAsyncMethod, Process};
use crate::scheduler::signal as signal_sched;
use crate::stack::total_stack_size;
use crate::state::{RcState, State};
use rustix::param::page_size;
use std::ffi::CStr;
use std::slice;
use std::thread;

extern "C" {
    fn tzset();
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_new(
    argc: u32,
    argv: *const *const i8,
) -> *mut Runtime {
    // The first argument is the executable. Rust already supports fetching this
    // for us on all platforms, so we just discard it here and spare us having
    // to deal with any platform specifics.
    let mut args = Vec::with_capacity(argc as usize);

    if !argv.is_null() {
        for &ptr in slice::from_raw_parts(argv, argc as usize).iter().skip(1) {
            if ptr.is_null() {
                break;
            }

            args.push(CStr::from_ptr(ptr as _).to_string_lossy().into_owned());
        }
    }

    // Through FFI code we may end up using the system's time functions (e.g.
    // localtime_r()). These functions in turn may not call tzset(). Instead of
    // requiring an explicit call to tzset() every time, we call it here once.
    unsafe { tzset() };

    // We ignore all signals by default so they're routed to the signal handler
    // thread. This also takes care of ignoring SIGPIPE, which Rust normally
    // does for us when compiling an executable.
    signal_sched::block_all();

    // Configure the TLS provider. This must be done once before we start the
    // program.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to set up the default TLS cryptography provider");

    Box::into_raw(Box::new(Runtime::new(args)))
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_drop(runtime: *mut Runtime) {
    drop(Box::from_raw(runtime));
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_start(
    runtime: *mut Runtime,
    main_type: TypePointer,
    method: NativeAsyncMethod,
) {
    (*runtime).start(main_type, method);
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_state(
    runtime: *mut Runtime,
) -> *const State {
    (*runtime).state.as_ptr() as _
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_stack_mask(
    runtime: *mut Runtime,
) -> u64 {
    let raw_size = (&(*runtime)).state.config.stack_size;
    let total = total_stack_size(raw_size as _, page_size()) as u64;

    !(total - 1)
}

/// An Inko runtime along with all its state.
#[repr(C)]
pub struct Runtime {
    state: RcState,
}

impl Runtime {
    /// Returns a new `Runtime` instance.
    ///
    /// This method sets up the runtime and allocates the core types, but
    /// doesn't start any threads.
    fn new(args: Vec<String>) -> Self {
        Self { state: State::new(Config::from_env(), args) }
    }

    /// Starts the runtime using the given process and method as the entry
    /// point.
    ///
    /// This method blocks the current thread until the program terminates,
    /// though this thread itself doesn't run any processes (= it just
    /// waits/blocks until completion).
    fn start(&self, main_type: TypePointer, main_method: NativeAsyncMethod) {
        let state = self.state.clone();

        thread::Builder::new()
            .name("timeout".to_string())
            .spawn(move || state.timeout_worker.run(&state))
            .unwrap();

        for id in 0..self.state.network_pollers.len() {
            let state = self.state.clone();

            thread::Builder::new()
                .name(format!("netpoll {}", id))
                .spawn(move || NetworkPollerWorker::new(id, state).run())
                .unwrap();
        }

        // Signal handling is very racy, meaning that if we notify the signal
        // handler to shut down it may not observe the signal correctly,
        // resulting in the program hanging. To prevent this from happening, we
        // simply don't wait for the signal handler thread to stop during
        // shutdown.
        {
            let state = self.state.clone();

            thread::Builder::new()
                .name("signals".to_string())
                .spawn(move || signal_sched::Worker::new(state).run())
                .unwrap();
        }

        let main_proc = Process::main(main_type, main_method);

        self.state.scheduler.run(&self.state, main_proc);
    }
}
