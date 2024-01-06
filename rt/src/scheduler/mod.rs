pub mod process;
pub mod timeout_worker;
pub mod timeouts;

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

#[cfg(target_os = "linux")]
pub(crate) fn reset_affinity() {
    let mut set = CpuSet::new();

    for i in 0..CpuSet::MAX_CPU {
        set.set(i);
    }

    let _ = sched_setaffinity(Pid::from_raw(0), &set);
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn reset_affinity() {
    // Only implemented on Linux.
}
