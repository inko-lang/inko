//! Functions for performing garbage collection of a finished process.

use gc::profile::Profile;
use process::RcProcess;
use vm::state::RcState;

pub fn collect(vm_state: &RcState, process: &RcProcess, profile: &mut Profile) {
    process.reclaim_and_finalize(vm_state);
    profile.total.stop();
}
