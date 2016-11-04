//! Threads for garbage collecting memory.
use time;
use std::collections::{VecDeque, HashMap};

use immix::bucket::Bucket;
use object_pointer::ObjectPointer;
use process::RcProcess;
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
            self.mark_roots(process);
            self.mark_remembered_set(process);

            if collect_mature {
                self.finalize_all(process);
            } else {
                self.finalize_young(process);
            }

            process.increment_young_ages();

            self.update_collection_thresholds(process);
            self.reclaim_blocks(process, collect_mature);
            self.rewind_allocator(process, collect_mature);

            let duration = time::precise_time_ns() - start_time;

            println!("Finished GC run in {} ns ({} ms)",
                     duration,
                     (duration as f64) / 1000000.0);

            request.thread.reschedule(request.process.clone());
        }
    }

    /// Prepares the collection phase
    ///
    /// This will reset any line bitmaps and check if evacuation is required.
    fn prepare_collection(&self, process: &RcProcess, mature: bool) {
        let mut local_data = process.local_data_mut();

        for bucket in local_data.allocator.young_generation.iter_mut() {
            self.prepare_bucket(bucket);
        }

        if mature {
            self.prepare_bucket(&mut local_data.allocator.mature_generation);
        }
    }

    /// Prepares a single bucket for collection and evacuation (if needed).
    fn prepare_bucket(&self, bucket: &mut Bucket) {
        let mut available: isize = 0;
        let mut required: isize = 0;
        let evacuate = bucket.has_blocks_to_evacuate();

        // HashMap with the keys being the hole counts, and the values being the
        // indices of the corresponding blocks.
        let mut blocks_per_holes = HashMap::new();

        for (index, block) in bucket.blocks.iter_mut().enumerate() {
            if evacuate && block.holes > 0 {
                let count = block.available_lines_count();

                bucket.available_histogram.increment(block.holes, count);

                available += count as isize;

                blocks_per_holes.entry(block.holes)
                    .or_insert(Vec::new())
                    .push(index);
            }

            block.reset_bitmaps();
        }

        if available > 0 {
            let mut iter = bucket.mark_histogram.iter();

            while available > required {
                if let Some(bin) = iter.next() {
                    required += bucket.mark_histogram.get(bin).unwrap() as isize;

                    available -=
                        bucket.available_histogram.get(bin).unwrap() as isize;

                    // Mark all blocks with the matching number of holes as
                    // fragmented.
                    if let Some(indexes) = blocks_per_holes.get(&bin) {
                        for index in indexes {
                            bucket.blocks[*index].set_fragmented();
                        }
                    }
                } else {
                    break;
                }
            }
        }
    }

    /// Reclaims any unused blocks.
    fn reclaim_blocks(&self, process: &RcProcess, mature: bool) {
        let mut local_data = process.local_data_mut();

        local_data.allocator.reclaim_blocks(mature);
    }

    /// Rewinds the allocator to the first hole in every generation.
    fn rewind_allocator(&self, process: &RcProcess, mature: bool) {
        let mut local_data = process.local_data_mut();

        for bucket in local_data.allocator.young_generation.iter_mut() {
            bucket.rewind_allocator();
        }

        if mature {
            local_data.allocator.mature_generation.rewind_allocator();
        }
    }

    /// Marks all objects in the remembered set.
    fn mark_remembered_set(&self, process: &RcProcess) {
        let mut objects = VecDeque::new();
        let mut remembered_set = process.remembered_set_mut();

        for pointer in remembered_set.iter() {
            objects.push_back(pointer.as_raw_pointer());
        }

        self.mark_objects(process, objects);

        remembered_set.clear();
    }

    /// Requests and marks the set of roots.
    fn mark_roots(&self, process: &RcProcess) {
        self.mark_objects(process, process.roots());
    }

    /// Marks all the given objects, optionally evacuating them.
    fn mark_objects(&self,
                    process: &RcProcess,
                    mut objects: VecDeque<*const ObjectPointer>) {
        let mut local_data = process.local_data_mut();

        while objects.len() > 0 {
            let pointer_pointer = objects.pop_front().unwrap();

            let mut pointer =
                unsafe { &mut *(pointer_pointer as *mut ObjectPointer) };

            // TODO: unmarkable pointers should never be scheduled.
            if !pointer.is_markable() {
                continue;
            }

            let already_marked = pointer.is_marked();

            if pointer.should_promote_to_mature() {
                let promoted = self.promote_mature(process, pointer);

                objects.push_back(promoted.as_raw_pointer());

                continue;
            } else if pointer.should_evacuate() {
                let evacuated = self.evacuate(process, pointer);

                objects.push_back(evacuated.as_raw_pointer());

                continue;
            } else if pointer.is_forwarded() {
                pointer.resolve_forwarding_pointer();
            } else {
                pointer.mark();

                // Objects that are still reachable but should be finalized at
                // some point should be remembered so we don't accidentally
                // release their resources.
                if pointer.is_mature() {
                    local_data.allocator.mature_finalizer_set.retain(pointer);
                } else {
                    local_data.allocator.young_finalizer_set.retain(pointer);
                }
            }

            // Don't scan objects we have already scanned and marked before.
            if already_marked {
                continue;
            }

            for child_pointer_pointer in pointer.get().pointers() {
                let child_pointer = unsafe { &*child_pointer_pointer };

                if child_pointer.is_markable() && !child_pointer.is_marked() {
                    objects.push_back(child_pointer_pointer);
                }
            }
        }
    }

    /// Promotes an object to the mature generation.
    fn promote_mature(&self,
                      process: &RcProcess,
                      pointer: &mut ObjectPointer)
                      -> ObjectPointer {
        let mut local_data = process.local_data_mut();
        let mut old_obj = pointer.get_mut();
        let mut new_obj = old_obj.take();

        // When we allocate the object in the mature generation we insert the
        // pointer in the mature generation's finalizer set. As such we should
        // remove it from the young generation's set.
        local_data.allocator.young_finalizer_set.remove(pointer);

        new_obj.set_mature();

        let new_pointer = local_data.allocator.allocate_mature(new_obj);

        old_obj.forward_to(new_pointer);

        pointer.resolve_forwarding_pointer();

        new_pointer
    }

    // Evacuates a pointer.
    fn evacuate(&self,
                process: &RcProcess,
                pointer: &mut ObjectPointer)
                -> ObjectPointer {
        let mut local_data = process.local_data_mut();
        let is_mature = pointer.is_mature();

        // Remove the old pointer from the finalizer set so we don't end up
        // accidentally finalizing a evacuated object.
        if is_mature {
            local_data.allocator.mature_finalizer_set.remove(pointer);
        } else {
            local_data.allocator.young_finalizer_set.remove(pointer);
        };

        // When evacuating an object we must ensure we evacuate the object into
        // the same bucket.
        let mut bucket = pointer.block_mut().bucket_mut().unwrap();

        let mut old_obj = pointer.get_mut();
        let new_obj = old_obj.take();

        let (_, new_pointer) = local_data.allocator
            .allocate_bucket(bucket, new_obj);

        old_obj.forward_to(new_pointer);

        pointer.resolve_forwarding_pointer();

        if is_mature {
            local_data.allocator.mature_finalizer_set.insert(new_pointer);
        } else {
            local_data.allocator.young_finalizer_set.insert(new_pointer);
        }

        new_pointer
    }

    /// Finalizes unreachable young objects.
    fn finalize_young(&self, process: &RcProcess) {
        let mut local_data = process.local_data_mut();

        local_data.allocator.young_finalizer_set.finalize();
    }

    /// Finalizes unreachable objects from all generations.
    fn finalize_all(&self, process: &RcProcess) {
        let mut local_data = process.local_data_mut();

        local_data.allocator.young_finalizer_set.finalize();
        local_data.allocator.mature_finalizer_set.finalize();
    }

    fn update_collection_thresholds(&self, process: &RcProcess) {
        let mut local_data = process.local_data_mut();

        local_data.allocator.young_block_allocations = 0;
        local_data.allocator.mature_block_allocations = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiled_code::CompiledCode;
    use config::Config;
    use immix::bitmap::Bitmap;
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
        let global_alloc = GlobalAllocator::new();
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
    fn test_prepare_bucket_without_evacuation() {
        let (_perm_alloc, process) = process();
        let thread = gc_thread();
        let pointer = process.allocate_empty();

        pointer.mark();

        let mut block = pointer.block_mut();

        thread.prepare_bucket(block.bucket_mut().unwrap());

        // No evacuation needed means the available histogram is not updated.
        assert!(block.bucket().unwrap().available_histogram.get(1).is_none());

        assert!(block.used_lines_bitmap.is_empty());
        assert!(block.marked_objects_bitmap.is_empty());
    }

    #[test]
    fn test_prepare_bucket_with_evacuation() {
        let (_perm_alloc, process) = process();
        let thread = gc_thread();
        let pointer = process.allocate_empty();

        pointer.mark();
        pointer.block_mut().set_recyclable();

        let mut block = pointer.block_mut();

        // Normally the collector updates the mark histogram at the end of a
        // cycle. Since said code is not executed by the function we're testing
        // we'll update this histogram manually.
        block.bucket_mut().unwrap().mark_histogram.increment(1, 1);

        thread.prepare_bucket(block.bucket_mut().unwrap());

        assert_eq!(block.bucket().unwrap().available_histogram.get(1).unwrap(),
                   254);

        assert!(pointer.block().is_fragmented());
        assert!(pointer.block().used_lines_bitmap.is_empty());
        assert!(pointer.block().marked_objects_bitmap.is_empty());
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

        assert_eq!(allocator.eden_space_mut().blocks.len(), 1);
        assert_eq!(allocator.mature_generation_mut().blocks.len(), 1);

        assert_eq!(allocator.eden_space_mut().blocks[0].holes, 1);
        assert_eq!(allocator.mature_generation_mut().blocks[0].holes, 1);

        assert!(allocator.eden_space_mut().blocks[0].is_recyclable());
        assert!(allocator.mature_generation_mut().blocks[0].is_recyclable());
    }

    #[test]
    fn test_rewind_allocator_without_mature() {
        let (_perm_alloc, process) = process();
        let thread = gc_thread();
        let mut local_data = process.local_data_mut();
        let iterations = OBJECTS_PER_BLOCK - OBJECTS_PER_LINE;

        for i in 0..(iterations * 2) {
            let young = local_data.allocator.allocate_empty();

            let mature = local_data.allocator
                .allocate_mature(Object::new(object_value::none()));

            if i > iterations {
                young.mark();
                mature.mark();
            }
        }

        assert_eq!(local_data.allocator.young_generation[0].block_index, 1);
        assert_eq!(local_data.allocator.mature_generation.block_index, 1);

        thread.rewind_allocator(&process, false);

        assert_eq!(local_data.allocator.young_generation[0].block_index, 0);
        assert_eq!(local_data.allocator.mature_generation.block_index, 1);
    }

    #[test]
    fn test_rewind_allocator_with_mature() {
        let (_perm_alloc, process) = process();
        let thread = gc_thread();
        let mut local_data = process.local_data_mut();
        let iterations = OBJECTS_PER_BLOCK - OBJECTS_PER_LINE;

        for i in 0..(iterations * 2) {
            let young = local_data.allocator.allocate_empty();

            let mature = local_data.allocator
                .allocate_mature(Object::new(object_value::none()));

            if i > iterations {
                young.mark();
                mature.mark();
            }
        }

        assert_eq!(local_data.allocator.young_generation[0].block_index, 1);
        assert_eq!(local_data.allocator.mature_generation.block_index, 1);

        thread.rewind_allocator(&process, true);

        assert_eq!(local_data.allocator.young_generation[0].block_index, 0);
        assert_eq!(local_data.allocator.mature_generation.block_index, 0);
    }
}
