//! Threads for garbage collecting memory.
use time;
use rayon::prelude::*;

use gc::request::Request;
use immix::block::BYTES_PER_OBJECT;
use object::ObjectStatus;
use object_pointer::ObjectPointer;
use process::RcProcess;
use virtual_machine::RcVirtualMachineState;

/// Tuple containing the number of marked, evacuated, and promoted objects.
type TraceResult = (usize, usize, usize);

/// Macro used for conditionally moving objects or resolving forwarding
/// pointers.
macro_rules! move_object {
    ($bucket: expr, $pointer: expr, $status: ident, $body: expr) => ({
        let lock = $bucket.lock();

        match $pointer.status() {
            ObjectStatus::Resolve => $pointer.resolve_forwarding_pointer(),
            ObjectStatus::$status => $body,
            _ => {}
        }

        // Let's explicitly drop the lock for good measurement.
        drop(lock);
    });
}

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
        let move_objects = self.prepare_collection(process, collect_mature);

        let mark_start = time::precise_time_ns();
        let (marked, evacuated, promoted) = self.trace(process, move_objects);
        let mark_duration = time::precise_time_ns() - mark_start;

        process.increment_young_ages();

        self.update_collection_thresholds(process, collect_mature);
        self.reclaim_blocks(process, collect_mature);

        let duration = time::precise_time_ns() - start_time;
        let bytes = (marked + evacuated + promoted) * BYTES_PER_OBJECT;
        let mb_sec = ((bytes / 1024 / 1024) as f64 /
                      (duration as f64 / 1000000.0)) *
                     1000.0;

        println!("Finished GC (mature: {}) in {:.2} ms ({:.2} ms marking), {} \
                  marked, {} evacuated, {} promoted ({:.2} MB/sec)",
                 collect_mature,
                 (duration as f64) / 1000000.0,
                 (mark_duration as f64) / 1000000.0,
                 marked,
                 evacuated,
                 promoted,
                 mb_sec);

        request.thread.reschedule(request.process.clone());
    }

    /// Prepares all buckets for a collection cycle.
    ///
    /// This method returns true if objects have to be moved around, either due
    /// to evacuation or promotion.
    fn prepare_collection(&self, process: &RcProcess, mature: bool) -> bool {
        let mut local_data = process.local_data_mut();
        let mut move_objects = false;

        for bucket in local_data.allocator.young_generation.iter_mut() {
            if bucket.prepare_for_collection() {
                move_objects = true;
            }

            if bucket.promote {
                move_objects = true;
            }
        }

        let ref mut mature_space = local_data.allocator.mature_generation;

        if mature {
            if mature_space.prepare_for_collection() {
                move_objects = true;
            }
        } else {
            // Since the write barrier may track mature objects we need to
            // always reset mature bitmaps. This ensures we can scan said mature
            // objects for child pointers
            for mut block in mature_space.blocks.iter_mut() {
                block.reset_bitmaps();
            }
        }

        move_objects
    }

    /// Reclaims any unused blocks.
    fn reclaim_blocks(&self, process: &RcProcess, mature: bool) {
        let mut local_data = process.local_data_mut();

        local_data.allocator.reclaim_blocks(mature);
    }

    /// Traces through and marks all reachable objects.
    ///
    /// The return value is a tuple containing the following numbers:
    ///
    /// * The number of marked objects
    /// * The number of evacuated objects
    /// * The number of promoted objects
    fn trace(&self, process: &RcProcess, mut move_objects: bool) -> TraceResult {
        if process.local_data().remembered_set.len() > 0 &&
           self.trace_remembered_set(process) {
            move_objects = true;
        }

        if move_objects {
            self.trace_with_moving(process)
        } else {
            self.trace_without_moving(process)
        }
    }

    /// Traces through all pointers in the remembered set.
    ///
    /// Any young pointers found are promoted to the mature generation
    /// immediately. This removes the need for keeping track of pointers in the
    /// remembered set for a potential long amount of time.
    ///
    /// Returns true if any objects were promoted.
    fn trace_remembered_set(&self, process: &RcProcess) -> bool {
        let mut promoted = false;
        let mut pointers = Vec::new();

        for pointer in process.remembered_set_mut().drain() {
            pointers.push(pointer.pointer());
        }

        while let Some(pointer_pointer) = pointers.pop() {
            let mut pointer = pointer_pointer.get_mut();

            if pointer.is_mature() {
                pointer.get().push_pointers(&mut pointers);
            } else if pointer.is_young() {
                self.promote_mature(process, pointer);
                promoted = true;
            }
        }

        promoted
    }

    /// Traces through all objects without moving any.
    fn trace_without_moving(&self, process: &RcProcess) -> TraceResult {
        let marked = process.contexts()
            .par_iter()
            .weight_max()
            .map(|context| {
                let mut objects = context.pointers();
                let mut marked = 0;

                while let Some(pointer_pointer) = objects.pop() {
                    let pointer = pointer_pointer.get();

                    if pointer.is_marked() {
                        continue;
                    }

                    pointer.mark();

                    marked += 1;

                    pointer.get().push_pointers(&mut objects);
                }

                marked
            })
            .reduce(|| 0, |acc, curr| acc + curr);

        (marked, 0, 0)
    }

    /// Traces through all objects, evacuating or promoting them whenever
    /// needed.
    fn trace_with_moving(&self, process: &RcProcess) -> TraceResult {
        let local_data = process.local_data();
        let ref allocator = local_data.allocator;

        process.contexts()
            .par_iter()
            .weight_max()
            .map(|context| {
                let mut objects = context.pointers();
                let mut marked = 0;
                let mut evacuated = 0;
                let mut promoted = 0;

                while let Some(pointer_pointer) = objects.pop() {
                    let mut pointer = pointer_pointer.get_mut();

                    if pointer.is_marked() {
                        continue;
                    }

                    match pointer.status() {
                        ObjectStatus::Resolve => {
                            pointer.resolve_forwarding_pointer()
                        }
                        ObjectStatus::Promote => {
                            let ref bucket = allocator.mature_generation;

                            move_object!(bucket, pointer, Promote, {
                                self.promote_mature(process, pointer);

                                promoted += 1;
                            });
                        }
                        ObjectStatus::Evacuate => {
                            // To prevent borrow problems we first acquire a new
                            // reference to the pointer before locking its
                            // bucket.
                            let bucket =
                                pointer_pointer.get().block().bucket().unwrap();

                            move_object!(bucket, pointer, Evacuate, {
                                self.evacuate(process, pointer);

                                evacuated += 1;
                            });
                        }
                        _ => {}
                    }

                    pointer.mark();

                    marked += 1;

                    pointer.get().push_pointers(&mut objects);
                }

                (marked, evacuated, promoted)
            })
            .reduce(|| (0, 0, 0),
                    |acc, curr| (acc.0 + curr.0, acc.1 + curr.1, acc.2 + curr.2))
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
        let pointer = process.allocate_empty();

        assert_eq!(thread.prepare_collection(&process, true), false);

        pointer.block_mut().bucket_mut().unwrap().promote = true;

        assert_eq!(thread.prepare_collection(&process, true), true);
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
