//! Types and methods for scheduling garbage collection of a process.
use crate::gc::statistics::{CollectionStatistics, TraceStatistics};
use crate::gc::tracer::Pool;
use crate::mailbox::Mailbox;
use crate::process::RcProcess;
use crate::vm::state::State;
use std::time::Instant;

/// A garbage collection to perform.
pub struct Collection {
    /// The process that is being garbage collected.
    process: RcProcess,

    /// The time at which garbage collection started.
    start_time: Instant,
}

impl Collection {
    pub fn new(process: RcProcess) -> Self {
        Collection {
            process,
            start_time: Instant::now(),
        }
    }

    /// Starts garbage collecting the process.
    pub fn perform(
        &self,
        vm_state: &State,
        tracers: &Pool,
    ) -> CollectionStatistics {
        // We must lock the mailbox before performing any work, as otherwise new
        // objects may be allocated during garbage collection.
        let local_data = self.process.local_data_mut();
        let mut mailbox = local_data.mailbox.lock();
        let collect_mature = self.process.should_collect_mature_generation();
        let move_objects = self.process.prepare_for_collection(collect_mature);
        let trace_stats =
            self.trace(&mut mailbox, move_objects, collect_mature, tracers);

        self.process.reclaim_blocks(vm_state, collect_mature);

        // We drop the mutex guard before rescheduling so the process can
        // immediately start receiving messages again, and so it can send itself
        // messages.
        drop(mailbox);

        vm_state.scheduler.schedule(self.process.clone());

        let stats = CollectionStatistics {
            duration: self.start_time.elapsed(),
            trace: trace_stats,
        };

        if vm_state.config.print_gc_timings {
            eprintln!(
                "[{:#x}] GC (mature: {}) in {:?}, {} marked, {} promoted, {} evacuated",
                self.process.identifier(),
                collect_mature,
                stats.duration,
                stats.trace.marked,
                stats.trace.promoted,
                stats.trace.evacuated
            );
        }

        stats
    }

    /// Traces through and marks all reachable objects.
    fn trace(
        &self,
        mailbox: &mut Mailbox,
        move_objects: bool,
        mature: bool,
        tracers: &Pool,
    ) -> TraceStatistics {
        self.process
            .each_global_pointer(|ptr| tracers.schedule(ptr));

        mailbox.each_pointer(|ptr| tracers.schedule(ptr));

        if !mature {
            self.process
                .each_remembered_pointer(|ptr| tracers.schedule(ptr));
        }

        for context in self.process.contexts() {
            context.each_pointer(|ptr| tracers.schedule(ptr));
        }

        let stats = tracers.trace(&self.process, move_objects);

        if mature {
            self.process.prune_remembered_set();
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Binding;
    use crate::block::Block;
    use crate::config::Config;
    use crate::object::Object;
    use crate::object_pointer::ObjectPointer;
    use crate::object_value;
    use crate::vm::state::State;
    use crate::vm::test::setup;

    #[test]
    fn test_perform() {
        let (_machine, _block, process) = setup();
        let state = State::with_rc(Config::new(), &[]);
        let pointer = process.allocate_empty();
        let collection = Collection::new(process.clone());

        process.context_mut().set_register(0, pointer);

        let stats = collection.perform(&state, &Pool::new(1));

        assert_eq!(stats.trace.marked, 1);
        assert_eq!(stats.trace.evacuated, 0);
        assert_eq!(stats.trace.promoted, 0);

        assert!(pointer.is_marked());
    }

    #[test]
    fn test_trace_trace_without_moving_without_mature() {
        let (_machine, _block, process) = setup();
        let collection = Collection::new(process.clone());
        let young = process.allocate_empty();
        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.mark();

        process.context_mut().set_register(0, young);
        process.context_mut().set_register(1, mature);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            false,
            false,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 1);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_trace_without_moving_with_mature() {
        let (_machine, _block, process) = setup();
        let collection = Collection::new(process.clone());
        let young = process.allocate_empty();
        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        process.context_mut().set_register(0, young);
        process.context_mut().set_register(1, mature);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            false,
            true,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 2);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_trace_with_moving_without_mature() {
        let (_machine, _block, process) = setup();
        let collection = Collection::new(process.clone());
        let young = process.allocate_empty();
        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.mark();

        young.block_mut().set_fragmented();

        process.context_mut().set_register(0, young);
        process.context_mut().set_register(1, mature);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            true,
            false,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 1);
        assert_eq!(stats.evacuated, 1);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_trace_with_moving_with_mature() {
        let (_machine, _block, process) = setup();
        let collection = Collection::new(process.clone());
        let young = process.allocate_empty();
        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        young.block_mut().set_fragmented();
        mature.block_mut().set_fragmented();

        process.context_mut().set_register(0, young);
        process.context_mut().set_register(1, mature);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            true,
            true,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 2);
        assert_eq!(stats.evacuated, 2);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_remembered_set_without_moving() {
        let (_machine, _block, process) = setup();
        let collection = Collection::new(process.clone());
        let local_data = process.local_data_mut();
        let pointer1 = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        local_data.allocator.remember_object(pointer1);

        process.prepare_for_collection(false);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            false,
            false,
            &Pool::new(1),
        );

        let remembered =
            local_data.allocator.remembered_set.iter().next().unwrap();

        assert!(remembered.is_marked());

        assert_eq!(stats.marked, 1);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_remembered_set_with_moving() {
        let (_machine, _block, process) = setup();
        let collection = Collection::new(process.clone());
        let local_data = process.local_data_mut();
        let pointer1 = local_data
            .allocator
            .allocate_mature(Object::new(object_value::float(4.5)));

        pointer1.block_mut().set_fragmented();

        local_data.allocator.remember_object(pointer1);

        process.prepare_for_collection(false);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            true,
            false,
            &Pool::new(1),
        );

        let remembered =
            local_data.allocator.remembered_set.iter().next().unwrap();

        assert_eq!(remembered.block().is_fragmented(), false);
        assert!(remembered.is_marked());
        assert!(remembered.float_value().is_ok());

        assert_eq!(stats.marked, 1);
        assert_eq!(stats.evacuated, 1);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_prune_remembered_set() {
        let (_machine, _block, process) = setup();
        let collection = Collection::new(process.clone());
        let local_data = process.local_data_mut();

        let pointer1 = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        let pointer2 = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        process.context_mut().set_register(0, pointer2);

        local_data.allocator.remember_object(pointer1);
        local_data.allocator.remember_object(pointer2);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            false,
            true,
            &Pool::new(1),
        );

        let mut iter = local_data.allocator.remembered_set.iter();

        assert!(iter.next() == Some(&pointer2));
        assert!(iter.next().is_none());

        assert_eq!(stats.marked, 1);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_mailbox_with_moving_without_mature() {
        let (_machine, _block, process) = setup();
        let young = process.allocate_empty();
        let collection = Collection::new(process.clone());
        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.mark();

        young.block_mut().set_fragmented();

        process.send_message_from_self(young);
        process.send_message_from_self(mature);
        process.prepare_for_collection(false);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            true,
            false,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 1);
        assert_eq!(stats.evacuated, 1);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_mailbox_with_moving_with_mature() {
        let (_machine, _block, process) = setup();
        let young = process.allocate_empty();
        let collection = Collection::new(process.clone());
        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        young.block_mut().set_fragmented();

        process.send_message_from_self(young);
        process.send_message_from_self(mature);
        process.prepare_for_collection(true);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            true,
            true,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 2);
        assert_eq!(stats.evacuated, 1);
        assert_eq!(stats.promoted, 0);

        assert!(mature.is_marked());
    }

    #[test]
    fn test_trace_mailbox_without_moving_without_mature() {
        let (_machine, _block, process) = setup();
        let young = process.allocate_empty();
        let collection = Collection::new(process.clone());
        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.mark();

        process.send_message_from_self(young);
        process.send_message_from_self(mature);
        process.prepare_for_collection(false);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            false,
            false,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 1);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);

        assert!(young.is_marked());
    }

    #[test]
    fn test_trace_mailbox_without_moving_with_mature() {
        let (_machine, _block, process) = setup();
        let young = process.allocate_empty();
        let collection = Collection::new(process.clone());
        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        process.send_message_from_self(young);
        process.send_message_from_self(mature);
        process.prepare_for_collection(true);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            false,
            true,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 2);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);

        assert!(young.is_marked());
        assert!(mature.is_marked());
    }

    #[test]
    fn test_trace_with_panic_handler() {
        let (_machine, block, process) = setup();
        let collection = Collection::new(process.clone());
        let local = process.allocate_empty();
        let receiver = process.allocate_empty();

        let code = process.context().code.clone();
        let binding = Binding::new(1, ObjectPointer::integer(1), None);

        binding.set_local(0, local);

        let new_block =
            Block::new(code, Some(binding), receiver, &block.module);

        let panic_handler =
            process.allocate_without_prototype(object_value::block(new_block));

        process.set_panic_handler(panic_handler);
        process.prepare_for_collection(false);

        let stats = collection.trace(
            &mut process.local_data_mut().mailbox.lock(),
            false,
            false,
            &Pool::new(1),
        );

        assert_eq!(stats.marked, 3);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);

        assert!(panic_handler.is_marked());
        assert!(receiver.is_marked());
        assert!(local.is_marked());
    }
}
