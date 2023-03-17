mod array;
mod byte_array;
mod class;
mod env;
mod float;
mod fs;
mod general;
mod hasher;
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
use std::thread;

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
    (*runtime).start(class, method);
}

#[no_mangle]
pub unsafe extern "system" fn inko_runtime_state(
    runtime: *mut Runtime,
) -> *const State {
    (*runtime).state.as_ptr() as _
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
        let config = Config::new();

        Self { state: State::new(config, counts, &[]) }
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

        self.state.scheduler.run(&*self.state, main_proc);
    }
}
