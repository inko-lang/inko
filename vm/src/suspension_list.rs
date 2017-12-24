//! Storing and processing of suspended VM processes.
//!
//! A SuspensionList can be used to track processes that are suspended for a
//! variety of reasons (e.g. because they're waiting for a message to arrive).

use std::cell::UnsafeCell;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, Condvar};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, Duration};

use process::RcProcess;
use vm::state::RcState;

pub struct SuspendedProcess {
    /// The process that is suspended.
    pub process: RcProcess,

    /// The time at which the process was suspended.
    pub suspended_at: Instant,

    /// The (optional) maximum amount of time the process should be suspended.
    pub timeout: Option<Duration>,
}

pub struct SuspensionList {
    /// The list of processes that should be suspended.
    pub outer: Mutex<HashSet<SuspendedProcess>>,

    /// The list of processes that are currently suspended.
    pub inner: UnsafeCell<HashSet<SuspendedProcess>>,

    /// Boolean that indicates if we should process the list.
    pub process: AtomicBool,

    /// A condition variable to signal whenever processes are to be
    /// suspended.
    pub condvar: Condvar,

    /// Set to true when we should wake up forcefully.
    pub wake_up: AtomicBool,
}

unsafe impl Sync for SuspensionList {}
unsafe impl Send for SuspensionList {}

impl PartialEq for SuspendedProcess {
    fn eq(&self, other: &SuspendedProcess) -> bool {
        self.process == other.process
    }
}

impl Eq for SuspendedProcess {}

impl Hash for SuspendedProcess {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.process.pid.hash(state);
    }
}

impl SuspendedProcess {
    pub fn new(process: RcProcess, timeout: Option<Duration>) -> Self {
        SuspendedProcess {
            process: process,
            suspended_at: Instant::now(),
            timeout: timeout,
        }
    }

    /// Returns `true` if the current entry's process should be rescheduled for
    /// execution.
    pub fn should_reschedule(&self) -> bool {
        let waiting_for_message = self.process.is_waiting_for_message();

        if waiting_for_message && self.process.has_messages() {
            return true;
        }

        if let Some(timeout) = self.timeout {
            let resume_after = self.suspended_at + timeout;

            if Instant::now() >= resume_after {
                self.process.wakeup_after_suspension_timeout();

                true
            } else {
                false
            }
        } else {
            true
        }
    }
}

impl SuspensionList {
    pub fn new() -> Self {
        SuspensionList {
            inner: UnsafeCell::new(HashSet::new()),
            outer: Mutex::new(HashSet::new()),
            process: AtomicBool::new(true),
            condvar: Condvar::new(),
            wake_up: AtomicBool::new(false),
        }
    }

    /// Suspends the given process, optionally with a timeout in milliseconds.
    pub fn suspend(&self, process: RcProcess, timeout: Option<u64>) {
        let entry = SuspendedProcess::new(
            process,
            timeout.map(|num| Duration::from_millis(num)),
        );

        lock!(self.outer).insert(entry);

        self.condvar.notify_all();
    }

    /// Notifies the suspension list that it should wake up.
    pub fn wake_up(&self) {
        self.condvar.notify_all();
        self.wake_up.store(true, Ordering::Relaxed);
    }

    pub fn reset_wake_up(&self) {
        self.wake_up.store(false, Ordering::Relaxed);
    }

    pub fn should_wake_up(&self) -> bool {
        self.wake_up.load(Ordering::Relaxed)
    }

    pub fn terminate(&self) {
        self.process.store(true, Ordering::Relaxed);
    }

    pub fn should_process(&self) -> bool {
        self.process.load(Ordering::Relaxed)
    }

    pub fn process_suspended_processes(&self, state: &RcState) {
        while self.should_process() {
            self.wait_for_work(state);
            self.copy_outer();
            self.process_inner(state);
        }
    }

    fn inner_mut(&self) -> &mut HashSet<SuspendedProcess> {
        unsafe { &mut *self.inner.get() }
    }

    fn inner(&self) -> &HashSet<SuspendedProcess> {
        unsafe { &*self.inner.get() }
    }

    fn copy_outer(&self) {
        let inner = self.inner_mut();
        let mut outer = lock!(self.outer);

        for entry in outer.drain() {
            inner.insert(entry);
        }
    }

    fn process_inner(&self, state: &RcState) {
        let inner = self.inner_mut();

        if inner.len() > 0 {
            inner.retain(|entry| if entry.should_reschedule() {
                state.process_pools.schedule(entry.process.clone());
                false
            } else {
                true
            });
        }
    }

    /// Blocks the current thread until either a process times out or new
    /// processes are scheduled for suspension.
    fn wait_for_work(&self, state: &RcState) {
        let sleep_for = self.time_to_sleep(state);
        let mut outer = lock!(self.outer);

        while outer.is_empty() {
            let result = self.condvar.wait_timeout(outer, sleep_for).unwrap();

            if result.1.timed_out() {
                return;
            } else if self.should_wake_up() {
                self.reset_wake_up();
                return;
            } else {
                outer = result.0;
            }
        }
    }

    /// Determine the amount of time we need to sleep before checking the list
    /// of processes again.
    fn time_to_sleep(&self, state: &RcState) -> Duration {
        let inner = self.inner();
        let mut time = None;

        // If there are processes with a timeout we want to make sure we resume
        // those as soon as possible after these timeouts expire.
        for entry in inner.iter() {
            if let Some(timeout) = entry.timeout {
                let overwrite = if let Some(current) = time {
                    timeout < current
                } else {
                    true
                };

                if overwrite {
                    time = Some(timeout);
                }
            }
        }

        if let Some(duration) = time {
            duration
        } else {
            Duration::from_millis(state.config.suspension_check_interval)
        }
    }
}
