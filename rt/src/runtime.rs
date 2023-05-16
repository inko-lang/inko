mod array;
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
use crate::scheduler::{number_of_cores, pin_thread_to_core};
use crate::stack::Stack;
use crate::state::{MethodCounts, RcState, State};
use std::env::args_os;
use std::io::{stdout, Write as _};
use std::process::exit as rust_exit;
use std::thread;

#[cfg(unix)]
fn ignore_sigpipe() {
    // Broken pipe errors default to terminating the entire program, making it
    // impossible to handle such errors. This is especially problematic for
    // sockets, as writing to a socket closed on the other end would terminate
    // the program.
    //
    // While Rust handles this for us when compiling an executable, it doesn't
    // do so when compiling it to a static library and linking it to our
    // generated code, so we must handle this ourselves.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
}

#[cfg(not(unix))]
fn ignore_sigpipe() {
    // Not needed on these platforms
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_new(
    counts: *mut MethodCounts,
) -> *mut Runtime {
    Box::into_raw(Box::new(Runtime::new(&*counts)))
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
    ignore_sigpipe();
    (*runtime).start(class, method);
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_state(
    runtime: *mut Runtime,
) -> *const State {
    (*runtime).state.as_ptr() as _
}

pub(crate) fn exit(status: i32) -> ! {
    // STDOUT is buffered by default, and not flushing it upon exit may result
    // in parent processes not observing the output.
    let _ = stdout().lock().flush();

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
    fn new(counts: &MethodCounts) -> Self {
        let config = Config::from_env();
        let args: Vec<_> = args_os()
            .skip(1)
            .map(|v| v.to_string_lossy().into_owned())
            .collect();

        Self { state: State::new(config, counts, &args) }
    }

    /// Starts the runtime using the given process and method as the entry
    /// point.
    ///
    /// This method blocks the current thread until the program terminates,
    /// though this thread itself doesn't run any processes (= it just
    /// waits/blocks until completion).
    fn start(&self, main_class: ClassPointer, main_method: NativeAsyncMethod) {
        let state = self.state.clone();
        let cores = number_of_cores();

        thread::Builder::new()
            .name("timeout".to_string())
            .spawn(move || {
                pin_thread_to_core(0);
                state.timeout_worker.run(&state.scheduler)
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
