pub mod process;
pub mod timeout_worker;
pub mod timeouts;

use std::thread::available_parallelism;

#[cfg(target_os = "linux")]
use rustix::process::{sched_setaffinity, CpuSet, Pid};

#[cfg(target_os = "linux")]
pub(crate) fn pin_thread_to_core(core: usize) {
    let mut set = CpuSet::new();
    set.set(core);

    let _ = sched_setaffinity(Pid::from_raw(0), &set);
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn pin_thread_to_core(_core: usize) {
    // Pinning is only implemented for Linux at this time.
}

pub(crate) fn number_of_cores() -> usize {
    available_parallelism().map(|v| v.into()).unwrap_or(1)
}
