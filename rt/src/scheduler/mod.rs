pub mod process;
pub mod timeout_worker;
pub mod timeouts;

use std::thread::available_parallelism;

#[cfg(target_os = "linux")]
use {
    libc::{cpu_set_t, sched_setaffinity, CPU_SET},
    std::mem::{size_of, zeroed},
};

#[cfg(target_os = "linux")]
pub(crate) fn pin_thread_to_core(core: usize) {
    unsafe {
        let mut set: cpu_set_t = zeroed();

        CPU_SET(core, &mut set);
        sched_setaffinity(0, size_of::<cpu_set_t>(), &set);
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn pin_thread_to_core(_core: usize) {
    // Pinning is only implemented for Linux at this time.
}

pub(crate) fn number_of_cores() -> usize {
    available_parallelism().map(|v| v.into()).unwrap_or(1)
}
