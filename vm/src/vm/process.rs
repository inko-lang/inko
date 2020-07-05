//! VM functions for working with Inko processes.
use crate::block::Block;
use crate::duration;
use crate::execution_context::ExecutionContext;
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::{Process, RcProcess, RescheduleRights};
use crate::scheduler::process_worker::ProcessWorker;
use crate::stacktrace;
use crate::vm::state::RcState;
use std::time::Duration;

pub fn local_exists(
    state: &RcState,
    context: &ExecutionContext,
    local: usize,
) -> ObjectPointer {
    if context.binding.local_exists(local) {
        state.true_object
    } else {
        state.false_object
    }
}

pub fn allocate(state: &RcState, block: &Block) -> RcProcess {
    Process::from_block(block, state.global_allocator.clone(), &state.config)
}

pub fn spawn(
    state: &RcState,
    current_process: &RcProcess,
    block_ptr: ObjectPointer,
    proto_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let block = block_ptr.block_value()?;
    let new_proc = allocate(&state, &block);

    // We schedule the process right away so we don't have to wait for the
    // allocation below (which may require requesting a new block) to finish.
    state.scheduler.schedule(new_proc.clone());

    let new_proc_ptr =
        current_process.allocate(object_value::process(new_proc), proto_ptr);

    Ok(new_proc_ptr)
}

pub fn send_message(
    state: &RcState,
    sender: &RcProcess,
    receiver_ptr: ObjectPointer,
    msg: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let receiver = receiver_ptr.process_value()?;

    if receiver == sender {
        receiver.send_message_from_self(msg);
    } else {
        receiver.send_message_from_external_process(msg);
        attempt_to_reschedule_process(state, &receiver);
    }

    Ok(msg)
}

pub fn receive_message(
    state: &RcState,
    process: &RcProcess,
) -> Option<ObjectPointer> {
    if let Some(msg) = process.receive_message() {
        process.no_longer_waiting_for_message();

        Some(msg)
    } else if process.is_waiting_for_message() {
        // A timeout expired, but no message was received.
        process.no_longer_waiting_for_message();

        Some(state.nil_object)
    } else {
        None
    }
}

pub fn wait_for_message(
    state: &RcState,
    process: &RcProcess,
    wait_for: Option<Duration>,
) {
    process.waiting_for_message();

    if let Some(duration) = wait_for {
        state.timeout_worker.suspend(process.clone(), duration);
    } else {
        process.suspend_without_timeout();
    }

    if process.has_messages() {
        // We may have received messages before marking the process as
        // suspended. If this happens we have to reschedule ourselves, otherwise
        // our process may be suspended until it is sent another message.
        attempt_to_reschedule_process(state, process);
    }
}

pub fn current_pid(state: &RcState, process: &RcProcess) -> ObjectPointer {
    process.allocate_usize(process.identifier(), state.integer_prototype)
}

pub fn suspend(
    state: &RcState,
    process: &RcProcess,
    wait_for: Option<Duration>,
) {
    if let Some(duration) = wait_for {
        state.timeout_worker.suspend(process.clone(), duration);
    } else {
        state.scheduler.schedule(process.clone());
    }
}

pub fn set_parent_local(
    context: &mut ExecutionContext,
    local: usize,
    depth: usize,
    value: ObjectPointer,
) -> Result<(), String> {
    if let Some(binding) = context.binding.find_parent_mut(depth) {
        binding.set_local(local, value);

        Ok(())
    } else {
        Err(format!("No binding for depth {}", depth))
    }
}

pub fn get_parent_local(
    context: &ExecutionContext,
    local: usize,
    depth: usize,
) -> Result<ObjectPointer, String> {
    if let Some(binding) = context.binding.find_parent(depth) {
        Ok(binding.get_local(local))
    } else {
        Err(format!("No binding for depth {}", depth))
    }
}

pub fn set_global(
    state: &RcState,
    context: &mut ExecutionContext,
    global: usize,
    object: ObjectPointer,
) -> ObjectPointer {
    let value = if object.is_permanent() {
        object
    } else {
        state.permanent_allocator.lock().copy_object(object)
    };

    context.set_global(global, value);

    value
}

pub fn stacktrace(
    state: &RcState,
    process: &RcProcess,
    limit_ptr: ObjectPointer,
    skip_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let limit = if limit_ptr == state.nil_object {
        None
    } else {
        Some(limit_ptr.usize_value()?)
    };

    let skip = skip_ptr.usize_value()?;

    Ok(stacktrace::allocate_stacktrace(process, state, limit, skip))
}

pub fn add_defer_to_caller(
    process: &RcProcess,
    block: ObjectPointer,
) -> Result<ObjectPointer, String> {
    if block.block_value().is_err() {
        return Err("only Blocks can be deferred".to_string());
    }

    let context = process.context_mut();

    // We can not use `if let Some(...) = ...` here as the
    // mutable borrow of "context" prevents the 2nd mutable
    // borrow inside the "else".
    if context.parent().is_some() {
        context.parent_mut().unwrap().add_defer(block);
    } else {
        context.add_defer(block);
    }

    Ok(block)
}

pub fn pin_thread(
    state: &RcState,
    process: &RcProcess,
    worker: &mut ProcessWorker,
) -> ObjectPointer {
    let result = if process.thread_id().is_some() {
        state.false_object
    } else {
        process.set_thread_id(worker.id as u8);

        state.true_object
    };

    worker.enter_exclusive_mode();

    result
}

pub fn unpin_thread(
    state: &RcState,
    process: &RcProcess,
    worker: &mut ProcessWorker,
) -> ObjectPointer {
    process.unset_thread_id();
    worker.leave_exclusive_mode();

    state.nil_object
}

pub fn identifier(
    state: &RcState,
    current_process: &RcProcess,
    process_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let proc = process_ptr.process_value()?;
    let proto = state.string_prototype;
    let identifier = current_process.allocate_usize(proc.identifier(), proto);

    Ok(identifier)
}

pub fn unwind_until_defining_scope(process: &RcProcess) {
    let top_binding = process.context().top_binding_pointer();

    loop {
        let context = process.context();

        if context.binding_pointer() == top_binding {
            return;
        } else {
            process.pop_context();
        }
    }
}

pub fn optional_timeout(
    pointer: ObjectPointer,
) -> Result<Option<Duration>, String> {
    duration::from_f64(pointer.float_value()?)
}

/// Attempts to reschedule the given process after it was sent a message.
fn attempt_to_reschedule_process(state: &RcState, process: &RcProcess) {
    // The logic below is necessary as a process' state may change between
    // sending it a message and attempting to reschedule it. Imagine we have two
    // processes: A, and B. A sends B a message, and B waits for a message twice
    // in a row. Now imagine the order of operations to be as follows:
    //
    //     Process A    | Process B
    //     -------------+--------------
    //     send(X)      | receive₁() -> X
    //                  | receive₂()
    //     reschedule() |
    //
    // The second receive() happens before we check the receiver's state to
    // determine if we can reschedule it. As a result we observe the process to
    // be suspended, and would attempt to reschedule it. Without checking if
    // this is actually still necessary, we would wake up the receiving process
    // too early, resulting the second receive() producing a nil object:
    //
    //     Process A    | Process B
    //     -------------+--------------
    //     send(X)      | receive₁() -> X
    //                  | receive₂() -> suspends
    //     reschedule() |
    //                  | receive₂() -> nil
    //
    // The logic below ensures that we only wake up a process when actually
    // necessary, and suspend it again if it didn't receive any messages (taking
    // into account messages it may have received while doing so).
    let reschedule = match process.acquire_rescheduling_rights() {
        RescheduleRights::Failed => false,
        RescheduleRights::Acquired => {
            if process.has_messages() {
                true
            } else {
                process.suspend_without_timeout();

                if process.has_messages() {
                    process.acquire_rescheduling_rights().are_acquired()
                } else {
                    false
                }
            }
        }
        RescheduleRights::AcquiredWithTimeout(timeout) => {
            if process.has_messages() {
                state.timeout_worker.increase_expired_timeouts();
                true
            } else {
                process.suspend_with_timeout(timeout);

                if process.has_messages() {
                    if process.acquire_rescheduling_rights().are_acquired() {
                        state.timeout_worker.increase_expired_timeouts();

                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    };

    if reschedule {
        state.scheduler.schedule(process.clone());
    }
}
