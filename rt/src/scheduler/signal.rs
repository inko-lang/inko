use crate::process::ProcessPointer;
use crate::state::RcState;
use libc::{
    pthread_sigmask, raise, sigaddset, sigdelset, sigfillset, signal, sigset_t,
    sigwait, SIGBUS, SIGCHLD, SIGCONT, SIGILL, SIGPIPE, SIGSEGV, SIGWINCH,
    SIG_IGN, SIG_SETMASK,
};
use std::collections::HashMap;
use std::ffi::c_int;
use std::mem::MaybeUninit;
use std::ptr::null_mut;
use std::sync::Mutex;

/// The signals we _shouldn't_ mask for application threads.
///
/// These signals should _not_ be masked, otherwise a program may hang when
/// encountering an error. For example, on macOS a stack overflow triggers a
/// SIGBUS but if this signal is masked the program just freezes.
const UNMASKED: [c_int; 3] = [SIGSEGV, SIGBUS, SIGILL];

/// Signals that some platforms may discard entirely by default and thus should
/// be re-enabled.
///
/// Most notably macOS discards these signals by default.
const ENABLE: [c_int; 3] = [SIGCHLD, SIGCONT, SIGWINCH];

extern "system" fn noop_signal_handler(_ignore: i32) {}

struct SignalSet {
    raw: sigset_t,
}

impl SignalSet {
    fn full() -> SignalSet {
        let raw = unsafe {
            let mut raw = MaybeUninit::uninit();

            sigfillset(raw.as_mut_ptr());
            raw.assume_init()
        };

        let mut set = SignalSet { raw };

        for sig in UNMASKED {
            set.remove(sig);
        }
        set
    }

    fn add(&mut self, signal: c_int) {
        unsafe {
            sigaddset(&mut self.raw as *mut _, signal);
        }
    }

    fn remove(&mut self, signal: c_int) {
        unsafe {
            sigdelset(&mut self.raw as *mut _, signal);
        }
    }

    fn wait(&self) -> c_int {
        let mut signal: c_int = 0;

        unsafe {
            sigwait(&self.raw as *const _, &mut signal as *mut _);
        }

        signal
    }

    fn block(&self) {
        unsafe {
            pthread_sigmask(SIG_SETMASK, &self.raw as *const _, null_mut());
        }
    }
}

/// Sets up the global signal handling configuration that applies to all
/// threads.
pub(crate) fn setup() {
    // Ensure all signals are ignored by default.
    SignalSet::full().block();

    // Ensure that if a SIGPIPE is somehow triggered _just_ after a sigwait() we
    // don't do anything with the signal.
    unsafe {
        signal(SIGPIPE, SIG_IGN);
    }

    for &sig in ENABLE.iter() {
        unsafe { signal(sig, noop_signal_handler as _) };
    }
}

pub(crate) struct Signals {
    waiting: Mutex<HashMap<c_int, Vec<ProcessPointer>>>,
}

impl Signals {
    pub(crate) fn new() -> Signals {
        Signals { waiting: Mutex::new(HashMap::new()) }
    }

    pub(crate) fn register(&self, process: ProcessPointer, signal: c_int) {
        self.waiting.lock().unwrap().entry(signal).or_default().push(process);
    }
}

pub(crate) struct Worker {
    state: RcState,
}

impl Worker {
    pub(crate) fn new(state: RcState) -> Worker {
        Worker { state }
    }

    pub(crate) fn run(&mut self) {
        // We mask/block all signals by default as different platforms handle
        // not doing so differently. For example, when _not_ blocking a signal
        // and calling sigwait(), Linux will still invoke the default signal
        // handler but macOS ignores the signal entirely.
        let mut set = SignalSet::full();

        loop {
            let sig = set.wait();

            // We mask SIGPIPE so it won't get triggered between now and the
            // next wait(), which could invoke its default behavior and cause
            // unexpected results.
            if sig == SIGPIPE {
                continue;
            }

            let mut procs = self.state.signals.waiting.lock().unwrap();

            if let Some(v) = procs.get_mut(&sig).filter(|v| !v.is_empty()) {
                self.state.scheduler.schedule_multiple(v);
            } else {
                drop(procs);

                // Invoke the default signal handler installed by the OS, which
                // we can't retrieve using e.g. sigaction().
                set.remove(sig);
                set.block();
                unsafe { raise(sig) };

                // If the default action doesn't terminate the program, add the
                // signal back to the set so we can wait for it again.
                set.add(sig);
                set.block();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libc::sigismember;

    #[test]
    fn test_signal_set_full() {
        let set = SignalSet::full();

        for sig in UNMASKED {
            assert_eq!(unsafe { sigismember(&set.raw, sig) }, 0);
        }
    }
}
