mod byte_array;
mod class;
mod env;
mod float;
mod fs;
mod general;
mod helpers;
mod int;
mod process;
mod random;
mod socket;
mod stdio;
mod string;
mod sys;
mod time;

use crate::config::Config;
use crate::mem::ClassPointer;
use crate::network_poller::Worker as NetworkPollerWorker;
use crate::process::{NativeAsyncMethod, Process};
use crate::scheduler::{pin_thread_to_core, reset_affinity};
use crate::stack::Stack;
use crate::state::{MethodCounts, RcState, State};
use std::ffi::CStr;
use std::io::{stdout, Write as _};
use std::process::exit as rust_exit;
use std::slice;
use std::thread;

const SIGPIPE: i32 = 13;
const SIG_IGN: usize = 1;

extern "C" {
    // Broken pipe errors default to terminating the entire program, making it
    // impossible to handle such errors. This is especially problematic for
    // sockets, as writing to a socket closed on the other end would terminate
    // the program.
    //
    // While Rust handles this for us when compiling an executable, it doesn't
    // do so when compiling it to a static library and linking it to our
    // generated code, so we must handle this ourselves.
    fn signal(sig: i32, handler: usize) -> usize;
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_new(
    counts: *mut MethodCounts,
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

    // The scheduler pins threads to specific cores. If those threads spawn a
    // new Inko process, those processes inherit the affinity and thus are
    // pinned to the same thread. This also result in Rust's
    // `available_parallelism()` function reporting 1, instead of e.g. 8 on a
    // system with 8 cores/threads.
    //
    // To fix this, we first reset the affinity so the default/current mask
    // allows use of all available cores/threads.
    reset_affinity();

    Box::into_raw(Box::new(Runtime::new(&*counts, args)))
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_drop(runtime: *mut Runtime) {
    drop(Box::from_raw(runtime));
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_start(
    runtime: *mut Runtime,
    class: ClassPointer,
    method: NativeAsyncMethod,
) {
    signal(SIGPIPE, SIG_IGN);
    (*runtime).start(class, method);
    flush_stdout();
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_state(
    runtime: *mut Runtime,
) -> *const State {
    (*runtime).state.as_ptr() as _
}

fn flush_stdout() {
    // STDOUT is buffered by default, and not flushing it upon exit may result
    // in parent processes not observing the output.
    let _ = stdout().lock().flush();
}

pub(crate) fn exit(status: i32) -> ! {
    flush_stdout();
    rust_exit(status);
}

/// An Inko runtime along with all its state.
#[repr(C)]
pub struct Runtime {
    state: RcState,
}

impl Runtime {
    /// Returns a new `Runtime` instance.
    ///
    /// This method sets up the runtime and allocates the core classes, but
    /// doesn't start any threads.
    fn new(counts: &MethodCounts, args: Vec<String>) -> Self {
        Self { state: State::new(Config::from_env(), counts, args) }
    }

    /// Starts the runtime using the given process and method as the entry
    /// point.
    ///
    /// This method blocks the current thread until the program terminates,
    /// though this thread itself doesn't run any processes (= it just
    /// waits/blocks until completion).
    fn start(&self, main_class: ClassPointer, main_method: NativeAsyncMethod) {
        let cores = self.state.cores as usize;
        let state = self.state.clone();

        thread::Builder::new()
            .name("timeout".to_string())
            .spawn(move || {
                pin_thread_to_core(0);
                state.timeout_worker.run(&state)
            })
            .unwrap();

        for id in 0..self.state.network_pollers.len() {
            let state = self.state.clone();

            thread::Builder::new()
                .name(format!("netpoll {}", id))
                .spawn(move || {
                    pin_thread_to_core(1 % cores);
                    NetworkPollerWorker::new(id, state).run()
                })
                .unwrap();
        }

        let stack = Stack::new(self.state.config.stack_size as usize);
        let main_proc = Process::main(main_class, main_method, stack);

        self.state.scheduler.run(&self.state, main_proc);
    }
}
