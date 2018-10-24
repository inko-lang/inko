//! VM functions for working with Inko processes.
use block::Block;
use immix::copy_object::CopyObject;
use object_pointer::ObjectPointer;
use pool::Worker;
use process::{Process, ProcessStatus, RcProcess};
use stacktrace;
use vm::state::RcState;

pub fn local_exists(
    state: &RcState,
    process: &RcProcess,
    local: usize,
) -> ObjectPointer {
    if process.local_exists(local) {
        state.true_object
    } else {
        state.false_object
    }
}

pub fn allocate(
    state: &RcState,
    pool_id: u8,
    block: &Block,
) -> Result<RcProcess, String> {
    let mut process_table = write_lock!(state.process_table);

    let pid = process_table
        .reserve()
        .ok_or_else(|| "No PID could be reserved".to_string())?;

    let process = Process::from_block(
        pid,
        pool_id,
        block,
        state.global_allocator.clone(),
        &state.config,
    );

    process_table.map(pid, process.clone());

    Ok(process)
}

pub fn spawn(
    state: &RcState,
    pool_id_ptr: ObjectPointer,
    block_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let pool_id = pool_id_ptr.u8_value()?;
    let block_obj = block_ptr.block_value()?;
    let new_proc = allocate(&state, pool_id, block_obj)?;
    let new_pid = new_proc.pid;
    let pid_ptr = new_proc.allocate_usize(new_pid, state.integer_prototype);

    state.process_pools.schedule(new_proc);

    Ok(pid_ptr)
}

pub fn send_message(
    state: &RcState,
    process: &RcProcess,
    pid_ptr: ObjectPointer,
    msg_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let pid = pid_ptr.usize_value()?;

    if let Some(receiver) = read_lock!(state.process_table).get(pid) {
        receiver.send_message(&process, msg_ptr);

        if receiver.is_waiting_for_message() {
            state.suspension_list.wake_up();
        }
    }

    Ok(msg_ptr)
}

pub fn wait_for_message(
    state: &RcState,
    process: &RcProcess,
    timeout: Option<u64>,
) {
    process.waiting_for_message();

    state.suspension_list.suspend(process.clone(), timeout);
}

pub fn current_pid(state: &RcState, process: &RcProcess) -> ObjectPointer {
    process.allocate_usize(process.pid, state.integer_prototype)
}

pub fn status(
    state: &RcState,
    pid_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let pid = pid_ptr.usize_value()?;
    let table = read_lock!(state.process_table);

    let status = if let Some(receiver) = table.get(pid) {
        receiver.status_integer()
    } else {
        ProcessStatus::Finished as u8
    };

    Ok(ObjectPointer::integer(i64::from(status)))
}

pub fn suspend(state: &RcState, process: &RcProcess, timeout: Option<u64>) {
    process.suspended();

    state.suspension_list.suspend(process.clone(), timeout);
}

pub fn set_parent_local(
    process: &RcProcess,
    local: usize,
    depth: usize,
    value: ObjectPointer,
) -> Result<(), String> {
    if let Some(binding) = process.context().binding.find_parent(depth) {
        binding.set_local(local, value);

        Ok(())
    } else {
        Err(format!("No binding for depth {}", depth))
    }
}

pub fn get_parent_local(
    process: &RcProcess,
    local: usize,
    depth: usize,
) -> Result<ObjectPointer, String> {
    if let Some(binding) = process.context().binding.find_parent(depth) {
        Ok(binding.get_local(local))
    } else {
        Err(format!("No binding for depth {}", depth))
    }
}

pub fn set_global(
    state: &RcState,
    process: &RcProcess,
    global: usize,
    object: ObjectPointer,
) -> ObjectPointer {
    let value = if object.is_permanent() {
        object
    } else {
        state.permanent_allocator.lock().copy_object(object)
    };

    process.set_global(global, value);

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
    worker: &mut Worker,
) -> ObjectPointer {
    let result = if process.thread_id().is_some() {
        state.false_object
    } else {
        process.set_thread_id(worker.thread_id);

        state.true_object
    };

    worker.pin();

    result
}

pub fn unpin_thread(
    state: &RcState,
    process: &RcProcess,
    worker: &mut Worker,
) -> ObjectPointer {
    process.unset_thread_id();

    worker.unpin();

    state.nil_object
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

pub fn optional_timeout(pointer: ObjectPointer) -> Option<u64> {
    if let Ok(time) = pointer.integer_value() {
        if time > 0 {
            Some(time as u64)
        } else {
            None
        }
    } else {
        None
    }
}
