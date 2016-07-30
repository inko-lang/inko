//! Threads for garbage collecting memory.

use virtual_machine::RcVirtualMachineState;

/// Structure containing the state of a single GC thread.
pub struct Thread {
    pub vm_state: RcVirtualMachineState,
}

impl Thread {
    pub fn new(vm_state: RcVirtualMachineState) -> Thread {
        Thread { vm_state: vm_state }
    }

    pub fn run(&mut self) {
        loop {
            let request = self.vm_state.gc_requests.pop();

            // If the process finished execution in the mean time we don't need
            // to run a GC cycle for it.
            if request.process.is_alive() {
                return;
            }

            // At this point either the process is still running _or_ it was
            // still running when we performed the above check. In this case
            // we'll just perform a collection. This means we may sometimes end
            // up collecting a dead process, but the above check should prevent
            // this from happening in most of the (trivial) cases.

            // 1: Install/enable write barrier
            // 2: Build and process worklist of pointers
            // 3: Defragmentation
            // 4: Moving objects
            // 5: Install forwarding pointers
        }
    }
}
