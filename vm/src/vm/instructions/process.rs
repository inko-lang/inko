//! VM functions for working with Inko processes.
use crate::block::Block;
use crate::duration;
use crate::execution_context::ExecutionContext;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::{Process, RcProcess, RescheduleRights};
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::state::RcState;

#[inline(always)]
pub fn process_allocate(state: &RcState, block: &Block) -> RcProcess {
    Process::from_block(block, state.global_allocator.clone(), &state.config)
}

#[inline(always)]
pub fn process_set_panic_handler(
    process: &RcProcess,
    handler: ObjectPointer,
) -> ObjectPointer {
    process.set_panic_handler(handler);
    handler
}

#[inline(always)]
pub fn process_current(state: &RcState, process: &RcProcess) -> ObjectPointer {
    process.allocate(
        object_value::process(process.clone()),
        state.process_prototype,
    )
}

#[inline(always)]
pub fn process_spawn(
    state: &RcState,
    current_process: &RcProcess,
    block_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let block = block_ptr.block_value()?;
    let new_proc = process_allocate(&state, &block);

    // We schedule the process right away so we don't have to wait for the
    // allocation below (which may require requesting a new block) to finish.
    state.scheduler.schedule(new_proc.clone());

    let new_proc_ptr = current_process
        .allocate(object_value::process(new_proc), state.process_prototype);

    Ok(new_proc_ptr)
}

#[inline(always)]
pub fn process_send_message(
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

#[inline(always)]
pub fn process_receive_message(
    state: &RcState,
    process: &RcProcess,
) -> Result<Option<ObjectPointer>, ObjectPointer> {
    if let Some(msg) = process.receive_message() {
        process.no_longer_waiting_for_message();

        Ok(Some(msg))
    } else if process.is_waiting_for_message() {
        // A timeout expired, but no message was received.
        process.no_longer_waiting_for_message();

        Err(state.intern_string("The timeout expired".to_string()))
    } else {
        Ok(None)
    }
}

#[inline(always)]
pub fn wait_for_message(
    state: &RcState,
    process: &RcProcess,
    timeout_ptr: ObjectPointer,
) -> Result<(), String> {
    let wait_for = duration::from_f64(timeout_ptr.float_value()?)?;

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

    Ok(())
}

#[inline(always)]
pub fn process_suspend_current(
    state: &RcState,
    process: &RcProcess,
    timeout_ptr: ObjectPointer,
) -> Result<(), String> {
    let wait_for = duration::from_f64(timeout_ptr.float_value()?)?;

    if let Some(duration) = wait_for {
        state.timeout_worker.suspend(process.clone(), duration);
    } else {
        state.scheduler.schedule(process.clone());
    }

    Ok(())
}

#[inline(always)]
pub fn process_set_blocking(
    state: &RcState,
    process: &RcProcess,
    blocking_ptr: ObjectPointer,
) -> ObjectPointer {
    let is_blocking = blocking_ptr == state.true_object;

    if process.is_pinned() || is_blocking == process.is_blocking() {
        // If a process is pinned we can't move it to another pool. We can't
        // panic in this case, since it would prevent code from using certain IO
        // operations that may try to move the process to another pool.
        //
        // Instead, we simply ignore the request and continue running on the
        // current thread.
        state.false_object
    } else {
        process.set_blocking(is_blocking);
        state.true_object
    }
}

#[inline(always)]
pub fn stacktrace(
    state: &RcState,
    process: &RcProcess,
    limit_ptr: ObjectPointer,
    skip_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let limit = limit_ptr.usize_value()?;
    let skip = skip_ptr.usize_value()?;

    Ok(allocate_stacktrace(process, state, limit, skip))
}

#[inline(always)]
pub fn process_add_defer_to_caller(
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

#[inline(always)]
pub fn process_set_pinned(
    state: &RcState,
    process: &RcProcess,
    worker: &mut ProcessWorker,
    pinned: ObjectPointer,
) -> ObjectPointer {
    if pinned == state.true_object {
        let result = if process.thread_id().is_some() {
            state.false_object
        } else {
            process.set_thread_id(worker.id as u8);
            state.true_object
        };

        worker.enter_exclusive_mode();
        result
    } else {
        process.unset_thread_id();
        worker.leave_exclusive_mode();
        state.false_object
    }
}

#[inline(always)]
pub fn process_identifier(
    state: &RcState,
    current_process: &RcProcess,
    process_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let proc = process_ptr.process_value()?;
    let proto = state.string_prototype;
    let identifier = current_process.allocate_usize(proc.identifier(), proto);

    Ok(identifier)
}

#[inline(always)]
pub fn process_unwind_until_defining_scope(process: &RcProcess) {
    let top_binding = process.context().top_binding_pointer();

    loop {
        let context = process.context();

        if context.binding_pointer() == top_binding || process.pop_context() {
            return;
        }
    }
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

/// Produces a stacktrace containing up to N stack frames.
fn allocate_stacktrace(
    process: &RcProcess,
    state: &RcState,
    limit: usize,
    skip: usize,
) -> ObjectPointer {
    let mut trace = if limit > 0 {
        Vec::with_capacity(limit)
    } else {
        Vec::new()
    };

    let mut contexts: Vec<&ExecutionContext> = {
        let iter = process.contexts().into_iter().skip(skip);

        if limit > 0 {
            iter.take(limit).collect()
        } else {
            iter.collect()
        }
    };

    contexts.reverse();

    for context in contexts {
        let file = context.code.file;
        let name = context.code.name;
        let line = ObjectPointer::integer(i64::from(context.line()));
        let tuple = process.allocate(
            object_value::array(vec![file, name, line]),
            state.array_prototype,
        );

        trace.push(tuple);
    }

    process.allocate(object_value::array(trace), state.array_prototype)
}
