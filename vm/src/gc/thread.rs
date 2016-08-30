//! Threads for garbage collecting memory.
use time;
use std::collections::VecDeque;
use std::ptr;

use gc::request::Request;
use immix::block;

use object_pointer::ObjectPointer;
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
            // to run a GC cycle for it. Once we pass this check the process may
            // still finish prior to collection. This check is simply in place
            // to prevent collecting a process that finished before handling the
            // current GC request.
            if !request.process.is_alive() {
                return;
            }

            let start_time = time::precise_time_ns();

            request.process.request_gc_suspension();

            // Do we need to evacuate any objects?
            // ...
            let evacuate = false;

            self.mark_roots(&request, evacuate);
            self.mark_remembered_set(&request, evacuate);

            // Sweep & age objects
            if request.generation.is_young() {
                self.sweep_young(&request);
            } else {
                // self.sweep_mature(&request);
            }

            self.increment_young_ages(&request);

            // Reset mark bits

            // Release/reset unused blocks
            // ...

            let duration = time::precise_time_ns() - start_time;

            println!("Finished GC run in {} ns ({} ms)",
                     duration,
                     (duration as f64) / 1000000.0);

            request.thread.reschedule(request.process);
        }
    }

    fn increment_young_ages(&self, request: &Request) {
        request.process.increment_young_ages();
    }

    /// Marks all objects in the remembered set.
    fn mark_remembered_set(&self, request: &Request, evacuate: bool) {
        let mut objects = VecDeque::new();
        let mut remembered_set = request.process.remembered_set_mut();

        for pointer in remembered_set.iter() {
            objects.push_back(*pointer);
        }

        remembered_set.clear();

        self.mark_objects(request, objects, evacuate);
    }

    /// Requests and marks the set of roots.
    fn mark_roots(&self, request: &Request, evacuate: bool) {
        let roots = request.process.roots();

        self.mark_objects(request, roots, evacuate);
    }

    /// Marks all the given objects, optionally evacuating them.
    fn mark_objects(&self,
                    request: &Request,
                    mut objects: VecDeque<ObjectPointer>,
                    evacuate: bool) {
        while objects.len() > 0 {
            let pointer = objects.pop_front().unwrap();

            if pointer.should_promote_to_mature() {
                pointer.mark();
                pointer.mark_line();
                // let promoted = self.promote_mature(request, pointer);

                // objects.push_back(promoted);
            } else if evacuate {
                // TODO: object evacuation
            } else {
                pointer.mark();
                pointer.mark_line();
            }

            for child_pointer in pointer.get().pointers() {
                if child_pointer.is_markable() && !child_pointer.is_marked() {
                    objects.push_back(child_pointer);
                }
            }
        }
    }

    /// Promotes an object to the mature generation.
    fn promote_mature(&self,
                      request: &Request,
                      pointer: ObjectPointer)
                      -> ObjectPointer {
        let mut old_obj = pointer.get_mut();
        let mut new_obj = old_obj.take();

        new_obj.set_mature();

        let (new_pointer, _) =
            request.process.local_data_mut().allocator.allocate_mature(new_obj);

        old_obj.forward_to(new_pointer);

        new_pointer
    }

    /// Removes any unreachable objects from the young generation
    fn sweep_young(&self, request: &Request) {
        request.process.each_unmarked_young_pointer(|pointer| {
            let mut object = unsafe { &mut *pointer };

            // TODO: support destructors/finalization
            // object.deallocate_pointers();
            //
            // unsafe {
            //     ptr::drop_in_place(pointer);
            //     ptr::write_bytes(pointer, 0, block::BYTES_PER_OBJECT);
            // };
        });
    }
}
