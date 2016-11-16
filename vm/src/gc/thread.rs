//! Threads for garbage collecting memory.
use time;

use gc::request::Request;
use object_pointer::ObjectPointer;
use object::ObjectStatus;
use process::RcProcess;
use virtual_machine::RcVirtualMachineState;
use immix::block::BYTES_PER_OBJECT;

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

            self.process_request(request);
        }
    }

    pub fn process_request(&self, request: Request) {
        let ref process = request.process;

        // If the process finished execution in the mean time we don't need
        // to run a GC cycle for it. Once we pass this check the process may
        // still finish prior to collection. This check is simply in place
        // to prevent collecting a process that finished before handling the
        // current GC request.
        if !process.is_alive() {
            return;
        }

        process.request_gc_suspension();

        let start_time = time::precise_time_ns();
        let collect_mature = process.should_collect_mature_generation();

        self.prepare_collection(process, collect_mature);

        let (marked, evacuated, promoted) = self.mark(process);

        process.increment_young_ages();

        self.update_collection_thresholds(process, collect_mature);
        self.reclaim_blocks(process, collect_mature);

        let duration = time::precise_time_ns() - start_time;
        let bytes = (marked + evacuated + promoted) * BYTES_PER_OBJECT;
        let mb_sec = ((bytes / 1024 / 1024) as f64 /
                      (duration as f64 / 1000000.0)) *
                     1000.0;

        println!("Finished GC (mature: {}) in {} ms, {} \
                  marked, {} evacuated, {} promoted ({:.2} MB/sec)",
                 collect_mature,
                 (duration as f64) / 1000000.0,
                 marked,
                 evacuated,
                 promoted,
                 mb_sec);

        request.thread.reschedule(request.process.clone());
    }

    fn prepare_collection(&self, process: &RcProcess, mature: bool) {
        let mut local_data = process.local_data_mut();

        for bucket in local_data.allocator.young_generation.iter_mut() {
            bucket.prepare_for_collection();
        }

        if mature {
            local_data.allocator.mature_generation.prepare_for_collection();
        }
    }

    /// Reclaims any unused blocks.
    fn reclaim_blocks(&self, process: &RcProcess, mature: bool) {
        let mut local_data = process.local_data_mut();

        local_data.allocator.reclaim_blocks(mature);
    }

    /// Marks all reachable objects.
    ///
    /// The return value is a tuple containing the following numbers:
    ///
    /// * The number of marked objects
    /// * The number of evacuated objects
    /// * The number of promoted objects
    fn mark(&self, process: &RcProcess) -> (usize, usize, usize) {
        let mut objects = process.roots();
        let mut remembered_set = process.remembered_set_mut();
        let mut marked = 0;
        let mut evacuated = 0;
        let mut promoted = 0;

        for pointer in remembered_set.iter() {
            objects.push(pointer.pointer());
        }

        while let Some(pointer_pointer) = objects.pop() {
            let mut pointer = pointer_pointer.get_mut();

            if pointer.is_marked() {
                continue;
            }

            match pointer.status() {
                ObjectStatus::Resolve => pointer.resolve_forwarding_pointer(),
                ObjectStatus::Promote => {
                    self.promote_mature(process, pointer);

                    promoted += 1;
                }
                ObjectStatus::Evacuate => {
                    self.evacuate(process, pointer);

                    evacuated += 1;
                }
                ObjectStatus::OK => {}
            }

            pointer.mark();

            marked += 1;

            pointer.get().push_pointers(&mut objects);
        }

        // The remembered set must be cleared _after_ traversing all objects as
        // we may otherwise invalidate pointers too early.
        remembered_set.clear();

        (marked, evacuated, promoted)
    }

    /// Promotes an object to the mature generation.
    ///
    /// The pointer to promote is updated to point to the new location.
    fn promote_mature(&self, process: &RcProcess, pointer: &mut ObjectPointer) {
        pointer.unmark_for_finalization();

        let mut local_data = process.local_data_mut();
        let mut old_obj = pointer.get_mut();
        let mut new_obj = old_obj.take();

        new_obj.set_mature();

        let new_pointer = local_data.allocator.allocate_mature(new_obj);

        old_obj.forward_to(new_pointer);

        pointer.resolve_forwarding_pointer();
    }

    // Evacuates a pointer.
    //
    // The pointer to evacuate is updated to point to the new location.
    fn evacuate(&self, process: &RcProcess, pointer: &mut ObjectPointer) {
        pointer.unmark_for_finalization();

        // When evacuating an object we must ensure we evacuate the object into
        // the same bucket.
        let local_data = process.local_data_mut();
        let mut bucket = pointer.block_mut().bucket_mut().unwrap();

        let mut old_obj = pointer.get_mut();
        let new_obj = old_obj.take();

        let (_, new_pointer) =
            bucket.allocate(&local_data.allocator.global_allocator, new_obj);

        old_obj.forward_to(new_pointer);

        pointer.resolve_forwarding_pointer();
    }

    fn update_collection_thresholds(&self, process: &RcProcess, mature: bool) {
        let mut local_data = process.local_data_mut();

        local_data.allocator.young_block_allocations = 0;

        if mature {
            local_data.allocator.mature_block_allocations = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiled_code::CompiledCode;
    use config::Config;
    use immix::block::{OBJECTS_PER_BLOCK, OBJECTS_PER_LINE};
    use immix::global_allocator::GlobalAllocator;
    use immix::permanent_allocator::PermanentAllocator;
    use object::Object;
    use object_value;
    use process::{Process, RcProcess};
    use virtual_machine::{VirtualMachineState, RcVirtualMachineState};

    fn vm_state() -> RcVirtualMachineState {
        VirtualMachineState::new(Config::new())
    }

    fn process() -> (PermanentAllocator, RcProcess) {
        let global_alloc = GlobalAllocator::without_preallocated_blocks();
        let mut perm_alloc = PermanentAllocator::new(global_alloc.clone());
        let self_obj = perm_alloc.allocate_empty();

        let code = CompiledCode::with_rc("a".to_string(),
                                         "a".to_string(),
                                         1,
                                         Vec::new());

        (perm_alloc, Process::from_code(1, code, self_obj, global_alloc))
    }

    fn gc_thread() -> Thread {
        Thread::new(vm_state())
    }

    #[test]
    fn test_prepare_collection() {
        let (_perm_alloc, process) = process();
        let thread = gc_thread();

        process.allocate_empty();

        // This is a smoke test to see if the code just runs. Most of the actual
        // logic resides in prepare_bucket() and is as such tested separately.
        thread.prepare_collection(&process, true);
    }

    #[test]
    fn test_reclaim_blocks() {
        let (_perm_alloc, process) = process();
        let thread = gc_thread();

        let iterations = OBJECTS_PER_BLOCK - OBJECTS_PER_LINE;
        let mut local_data = process.local_data_mut();
        let ref mut allocator = local_data.allocator;

        // Fill two blocks, the first one will be in use and the second one will
        // be treated as empty.
        for i in 0..(iterations * 2) {
            let young = allocator.allocate_empty();

            let mature =
                allocator.allocate_mature(Object::new(object_value::none()));

            // Mark objects 3..1020
            if i < iterations && i > 3 {
                young.mark();
                mature.mark();
            }
        }

        // This is to make sure that the assertions after calling
        // reclaim_blocks() don't pass because there was only 1 block.
        assert_eq!(allocator.eden_space_mut().blocks.len(), 2);
        assert_eq!(allocator.mature_generation_mut().blocks.len(), 2);

        thread.reclaim_blocks(&process, true);

        assert_eq!(allocator.eden_space_mut().blocks.len(), 0);
        assert_eq!(allocator.mature_generation.blocks.len(), 0);

        assert_eq!(allocator.eden_space_mut().recyclable_blocks[0].holes, 1);
        assert_eq!(allocator.mature_generation.recyclable_blocks[0].holes, 1);

        assert_eq!(allocator.eden_space_mut().recyclable_blocks.len(), 1);
        assert_eq!(allocator.mature_generation.recyclable_blocks.len(), 1);
    }
}
