//! Threads for garbage collecting memory.

use virtual_machine::RcVirtualMachineState;

/// Structure containing the state of a single GC thread.
pub struct GcThread {
    pub vm_state: RcVirtualMachineState,
}

impl GcThread {
    pub fn new(vm_state: RcVirtualMachineState) -> GcThread {
        GcThread { vm_state: vm_state }
    }

    pub fn run(&mut self) {
        loop {

        }
    }
}
