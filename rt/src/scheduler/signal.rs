use crate::process::ProcessPointer;
use crate::state::RcState;
use libc::{
    kill, pthread_sigmask, sigaddset, sigdelset, sigemptyset, sigfillset,
    sigset_t, sigwait, SIGPIPE, SIGURG, SIG_SETMASK,
};
use std::ffi::c_int;
use std::mem::MaybeUninit;
use std::process::id as pid;
use std::ptr::null_mut;
use std::sync::Mutex;

/// The signal to use to wake up the worker thread.
///
/// We use SIGURG as it's not commonly (if ever) used, isn't handled explicitly
/// by debuggers, and is ignored by default instead of terminating the program.
const NOTIFY: c_int = SIGURG;

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

        SignalSet { raw }
    }

    fn new() -> SignalSet {
        let raw = unsafe {
            let mut raw = MaybeUninit::uninit();

            sigemptyset(raw.as_mut_ptr());
            raw.assume_init()
        };

        let mut set = SignalSet { raw };

        set.add(NOTIFY);
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

/// Ignores the allowed signals sent to the current _process_.
pub(crate) fn block_all() {
    SignalSet::full().block()
}

/// Notifies the current process that the signals to wait for has changed.
pub(crate) fn notify_worker() {
    unsafe {
        // The thread used to register the process isn't the same as the
        // thread handling signals, so we use kill() here instead of e.g.
        // pthread_kill().
        kill(pid() as _, NOTIFY);
    }
}

pub(crate) struct Signals {
    waiting: Mutex<Vec<(ProcessPointer, c_int)>>,
}

impl Signals {
    pub(crate) fn new() -> Signals {
        Signals { waiting: Mutex::new(Vec::new()) }
    }

    pub(crate) fn register(&self, process: ProcessPointer, signal: c_int) {
        self.waiting.lock().unwrap().push((process, signal));
        notify_worker();
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
        {
            let mut blocked = SignalSet::new();

            // We explicitly block the notification signal such that if it's
            // sent before a wait() call we still observe it, and to make sure
            // the system's default behaviour isn't to e.g. terminate the
            // program.
            blocked.add(NOTIFY);

            // We block this signal because we don't allow handling it (as to
            // not mess with e.g. non-blocking sockets), and because in this
            // thread we don't want it to interrupt anything either.
            blocked.add(SIGPIPE);

            // For all other signals we retain the default behaviour, which is
            // usually to terminate the program.
            blocked.block();
        }

        let mut set = SignalSet::new();

        loop {
            // Because we block the NOTIFY signal, if it's sent _before_ this
            // call we still observe it. This isn't the case for other signals
            // though.
            let received = set.wait();
            let mut waiting = self.state.signals.waiting.lock().unwrap();

            if received != NOTIFY {
                let mut resched = Vec::new();

                // As the list of processes waiting for signals is typically
                // small (maybe a few at most), performing this linear scan
                // should take little time.
                waiting.retain(|(proc, desired)| {
                    if received == *desired {
                        resched.push(*proc);
                        false
                    } else {
                        true
                    }
                });

                // We remove the signal from the set as to invoke the default
                // behaviour for when the signal is received in the future.
                set.remove(received);
                self.state.scheduler.schedule_multiple(resched);
            }

            // This ensures that any remaining or newly added signals are part
            // of the set and masked for the current thread, such that our
            // wait() waits for them, instead of the default behaviour being
            // invoked.
            for (_, sig) in waiting.iter() {
                set.add(*sig);
                set.block();
            }
        }
    }
}
