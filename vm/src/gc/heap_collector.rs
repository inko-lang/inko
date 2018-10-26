//! Functions for performing garbage collection of a process heap.

use rayon::prelude::*;

use gc::collector;
use gc::profile::Profile;
use gc::trace_result::TraceResult;
use process::RcProcess;
use vm::state::RcState;

pub fn collect(vm_state: &RcState, process: &RcProcess, profile: &mut Profile) {
    let collect_mature = process.should_collect_mature_generation();

    profile.prepare.start();

    let move_objects = process.prepare_for_collection(collect_mature);

    profile.prepare.stop();
    profile.trace.start();

    let trace_result = trace(process, move_objects, collect_mature);

    profile.trace.stop();
    profile.reclaim.start();

    process.reclaim_blocks(vm_state, collect_mature);
    process.update_collection_statistics(collect_mature);

    profile.reclaim.stop();

    vm_state.process_pools.schedule(process.clone());

    profile.suspended.stop();

    profile.total.stop();
    profile.populate_tracing_statistics(&trace_result);
}

/// Traces through and marks all reachable objects.
pub fn trace(
    process: &RcProcess,
    move_objects: bool,
    mature: bool,
) -> TraceResult {
    let mut result = if move_objects {
        trace_mailbox_locals_with_moving(process, mature)
            + trace_with_moving(process, mature)
    } else {
        trace_mailbox_locals_without_moving(process, mature)
            + trace_without_moving(process, mature)
    };

    if mature {
        prune_remembered_set(process);
    } else if process.has_remembered_objects() {
        result = result + trace_remembered_set(process, move_objects);
    }

    result
}

/// Traces through all pointers in the remembered set.
///
/// Any young pointers found are promoted to the mature generation
/// immediately. This removes the need for keeping track of pointers in the
/// remembered set for a potential long amount of time.
///
/// Returns true if any objects were promoted.
pub fn trace_remembered_set(
    process: &RcProcess,
    move_objects: bool,
) -> TraceResult {
    let pointers = process.local_data().allocator.remembered_pointers();

    if move_objects {
        collector::trace_pointers_with_moving(process, pointers, true)
    } else {
        collector::trace_pointers_without_moving(pointers, true)
    }
}

/// Removes unmarked objects from the remembered set.
///
/// During a mature collection we don't examine the remembered set since we
/// already traverse all mature objects. This allows us to remove any
/// unmarked mature objects from the remembered set.
pub fn prune_remembered_set(process: &RcProcess) {
    process
        .local_data_mut()
        .allocator
        .prune_remembered_objects();
}

/// Traces through all local pointers in a mailbox, without moving objects.
pub fn trace_mailbox_locals_without_moving(
    process: &RcProcess,
    mature: bool,
) -> TraceResult {
    let local_data = process.local_data_mut();
    let objects = local_data.mailbox.local_pointers();

    collector::trace_pointers_without_moving(objects, mature)
}

/// Traces through all local pointers in a mailbox, potentially moving
/// objects.
pub fn trace_mailbox_locals_with_moving(
    process: &RcProcess,
    mature: bool,
) -> TraceResult {
    let local_data = process.local_data_mut();
    let objects = local_data.mailbox.local_pointers();

    collector::trace_pointers_with_moving(process, objects, mature)
}

/// Traces through all objects without moving any.
pub fn trace_without_moving(process: &RcProcess, mature: bool) -> TraceResult {
    let result = process
        .contexts()
        .par_iter()
        .map(|context| {
            collector::trace_pointers_without_moving(context.pointers(), mature)
        })
        .reduce(TraceResult::new, |acc, curr| acc + curr);

    result
        + collector::trace_pointers_without_moving(
            process.global_pointers_to_trace(),
            mature,
        )
}

/// Traces through the roots and all their child pointers, potentially
/// moving objects around.
pub fn trace_with_moving(process: &RcProcess, mature: bool) -> TraceResult {
    let result = process
        .contexts()
        .par_iter()
        .map(|context| {
            collector::trace_pointers_with_moving(
                process,
                context.pointers(),
                mature,
            )
        })
        .reduce(TraceResult::new, |acc, curr| acc + curr);

    result
        + collector::trace_pointers_with_moving(
            process,
            process.global_pointers_to_trace(),
            mature,
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use binding::Binding;
    use block::Block;
    use config::Config;
    use execution_context::ExecutionContext;
    use object::Object;
    use object_pointer::ObjectPointer;
    use object_value;
    use vm::state::State;
    use vm::test::setup;

    #[test]
    fn test_collect() {
        let (_machine, _block, process) = setup();
        let state = State::new(Config::new(), &[]);
        let pointer = process.allocate_empty();
        let mut profile = Profile::young();

        process.set_register(0, pointer);

        collect(&state, &process, &mut profile);

        assert_eq!(profile.marked, 1);
        assert_eq!(profile.evacuated, 0);
        assert_eq!(profile.promoted, 0);

        assert!(pointer.is_marked());
    }

    #[test]
    fn test_trace_trace_without_moving_without_mature() {
        let (_machine, _block, process) = setup();

        let young = process.allocate_empty();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        process.set_register(0, young);
        process.set_register(1, mature);

        let result = trace(&process, false, false);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_trace_without_moving_with_mature() {
        let (_machine, _block, process) = setup();

        let young = process.allocate_empty();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        process.set_register(0, young);
        process.set_register(1, mature);

        let result = trace(&process, false, true);

        assert_eq!(result.marked, 2);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_trace_with_moving_without_mature() {
        let (_machine, _block, process) = setup();

        let young = process.allocate_empty();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        young.block_mut().set_fragmented();

        process.set_register(0, young);
        process.set_register(1, mature);

        let result = trace(&process, true, false);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 1);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_trace_with_moving_with_mature() {
        let (_machine, _block, process) = setup();

        let young = process.allocate_empty();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        young.block_mut().set_fragmented();
        mature.block_mut().set_fragmented();

        process.set_register(0, young);
        process.set_register(1, mature);

        let result = trace(&process, true, true);

        assert_eq!(result.marked, 2);
        assert_eq!(result.evacuated, 2);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_remembered_set_without_moving() {
        let (_machine, _block, process) = setup();

        let local_data = process.local_data_mut();

        let pointer1 = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        local_data.allocator.remember_object(pointer1);

        process.prepare_for_collection(false);

        let result = trace_remembered_set(&process, false);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_remembered_set_with_moving() {
        let (_machine, _block, process) = setup();

        let local_data = process.local_data_mut();

        let pointer1 = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        pointer1.block_mut().set_fragmented();

        local_data.allocator.remember_object(pointer1);

        process.prepare_for_collection(false);

        let result = trace_remembered_set(&process, true);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 1);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_prune_remembered_set() {
        let (_machine, _block, process) = setup();

        let local_data = process.local_data_mut();

        let pointer1 = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        let pointer2 = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        pointer2.mark();

        local_data.allocator.remember_object(pointer1);
        local_data.allocator.remember_object(pointer2);

        prune_remembered_set(&process);

        assert_eq!(
            local_data.allocator.remembered_set.contains(&pointer1),
            false
        );

        assert!(local_data.allocator.remembered_set.contains(&pointer2));
    }

    #[test]
    fn test_trace_mailbox_locals_with_moving_without_mature() {
        let (_machine, _block, process) = setup();
        let young = process.allocate_empty();
        let local_data = process.local_data_mut();

        let mature = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        young.block_mut().set_fragmented();

        local_data.mailbox.send_from_self(young);
        local_data.mailbox.send_from_self(mature);

        process.prepare_for_collection(false);

        let result = trace_mailbox_locals_with_moving(&process, false);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 1);
        assert_eq!(result.promoted, 0);

        assert_eq!(mature.is_marked(), false);
    }

    #[test]
    fn test_trace_mailbox_locals_with_moving_with_mature() {
        let (_machine, _block, process) = setup();
        let young = process.allocate_empty();
        let local_data = process.local_data_mut();

        let mature = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        young.block_mut().set_fragmented();

        local_data.mailbox.send_from_self(young);
        local_data.mailbox.send_from_self(mature);

        process.prepare_for_collection(true);

        let result = trace_mailbox_locals_with_moving(&process, true);

        assert_eq!(result.marked, 2);
        assert_eq!(result.evacuated, 1);
        assert_eq!(result.promoted, 0);

        assert!(mature.is_marked());
    }

    #[test]
    fn test_trace_mailbox_locals_without_moving_without_mature() {
        let (_machine, _block, process) = setup();
        let young = process.allocate_empty();
        let local_data = process.local_data_mut();

        let mature = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        local_data.mailbox.send_from_self(young);
        local_data.mailbox.send_from_self(mature);

        process.prepare_for_collection(false);

        let result = trace_mailbox_locals_without_moving(&process, false);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);

        assert!(young.is_marked());
        assert_eq!(mature.is_marked(), false);
    }

    #[test]
    fn test_trace_mailbox_locals_without_moving_with_mature() {
        let (_machine, _block, process) = setup();
        let young = process.allocate_empty();
        let local_data = process.local_data_mut();

        let mature = local_data
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        local_data.mailbox.send_from_self(young);
        local_data.mailbox.send_from_self(mature);

        process.prepare_for_collection(true);

        let result = trace_mailbox_locals_without_moving(&process, true);

        assert_eq!(result.marked, 2);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);

        assert!(young.is_marked());
        assert!(mature.is_marked());
    }

    #[test]
    fn test_trace_without_moving_without_mature() {
        let (_machine, block, process) = setup();
        let pointer1 = process.allocate_empty();
        let pointer2 = process.allocate_empty();
        let pointer3 = process.allocate_empty();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        let receiver = process.allocate_empty();
        let code = process.context().code.clone();
        let new_block = Block::new(code, None, receiver, block.global_scope);
        let mut context = ExecutionContext::from_block(&new_block, None);

        context.add_defer(pointer3);
        process.set_register(0, pointer1);
        process.push_context(context);

        process.set_register(0, pointer2);
        process.set_register(1, mature);

        pointer1.block_mut().set_fragmented();

        process.prepare_for_collection(false);

        let result = trace_without_moving(&process, false);

        assert_eq!(result.marked, 4);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);

        assert_eq!(mature.is_marked(), false);
        assert!(receiver.is_marked());
        assert!(pointer3.is_marked());
    }

    #[test]
    fn test_trace_without_moving_with_panic_handler() {
        let (_machine, block, process) = setup();
        let local = process.allocate_empty();
        let receiver = process.allocate_empty();

        let code = process.context().code.clone();
        let binding = Binding::new(1, ObjectPointer::integer(1));

        binding.set_local(0, local);

        let new_block =
            Block::new(code, Some(binding), receiver, block.global_scope);

        let panic_handler =
            process.allocate_without_prototype(object_value::block(new_block));

        process.set_panic_handler(panic_handler);
        process.prepare_for_collection(false);

        let result = trace_without_moving(&process, false);

        assert_eq!(result.marked, 3);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);

        assert!(panic_handler.is_marked());
        assert!(receiver.is_marked());
        assert!(local.is_marked());
    }

    #[test]
    fn test_trace_with_moving_with_panic_handler() {
        let (_machine, block, process) = setup();
        let local = process.allocate_empty();
        let receiver = process.allocate_empty();

        let code = process.context().code.clone();
        let binding = Binding::new(1, ObjectPointer::integer(1));

        binding.set_local(0, local);

        let new_block =
            Block::new(code, Some(binding), receiver, block.global_scope);

        let panic_handler =
            process.allocate_without_prototype(object_value::block(new_block));

        receiver.block_mut().set_fragmented();

        process.set_panic_handler(panic_handler);
        process.prepare_for_collection(false);

        let result = trace_with_moving(&process, false);

        assert_eq!(result.marked, 3);
        assert_eq!(result.evacuated, 3);
        assert_eq!(result.promoted, 0);

        {
            let handler = process.panic_handler().unwrap();
            let block = handler.block_value().unwrap();

            assert!(handler.is_marked());

            assert!(block.receiver.is_marked());

            assert!(block
                .captures_from
                .as_ref()
                .unwrap()
                .get_local(0)
                .is_marked());
        }
    }

    #[test]
    fn test_trace_without_moving_with_mature() {
        let (_machine, block, process) = setup();
        let pointer1 = process.allocate_empty();
        let pointer2 = process.allocate_empty();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        let code = process.context().code.clone();
        let new_block = Block::new(
            code,
            None,
            ObjectPointer::integer(1),
            block.global_scope,
        );

        process.set_register(0, pointer1);

        process.push_context(ExecutionContext::from_block(&new_block, None));

        process.set_register(0, pointer2);
        process.set_register(1, mature);

        pointer1.block_mut().set_fragmented();

        process.prepare_for_collection(true);

        let result = trace_without_moving(&process, true);

        assert_eq!(result.marked, 3);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);

        assert!(mature.is_marked());
    }

    #[test]
    fn test_trace_with_moving_without_mature() {
        let (_machine, block, process) = setup();
        let pointer1 = process.allocate_empty();
        let pointer2 = process.allocate_empty();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        let code = process.context().code.clone();
        let new_block = Block::new(
            code,
            None,
            ObjectPointer::integer(1),
            block.global_scope,
        );

        process.set_register(0, pointer1);

        process.push_context(ExecutionContext::from_block(&new_block, None));

        process.set_register(0, pointer2);
        process.set_register(1, mature);

        pointer1.block_mut().set_fragmented();

        process.prepare_for_collection(false);

        let result = trace_with_moving(&process, false);

        assert_eq!(result.marked, 2);
        assert_eq!(result.evacuated, 2);
        assert_eq!(result.promoted, 0);

        assert_eq!(mature.is_marked(), false);
    }

    #[test]
    fn test_trace_with_moving_with_mature() {
        let (_machine, block, process) = setup();
        let pointer1 = process.allocate_empty();
        let pointer2 = process.allocate_empty();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        let code = process.context().code.clone();
        let new_block = Block::new(
            code,
            None,
            ObjectPointer::integer(1),
            block.global_scope,
        );

        process.set_register(0, pointer1);

        process.push_context(ExecutionContext::from_block(&new_block, None));

        process.set_register(0, pointer2);
        process.set_register(1, mature);

        pointer1.block_mut().set_fragmented();

        process.prepare_for_collection(true);

        let result = trace_with_moving(&process, true);

        assert_eq!(result.marked, 3);
        assert_eq!(result.evacuated, 2);
        assert_eq!(result.promoted, 0);

        assert!(mature.is_marked());
    }
}
