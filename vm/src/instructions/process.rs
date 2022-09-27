//! VM functions for working with Inko processes.
use crate::indexes::{ClassIndex, FieldIndex, MethodIndex};
use crate::mem::{Int, Pointer};
use crate::process::{
    Future, FutureState, Process, ProcessPointer, RescheduleRights,
    TaskPointer, Write, WriteResult,
};
use crate::scheduler::process::Thread;
use crate::scheduler::timeouts::Timeout;
use crate::state::State;
use std::time::Duration;

const SEND_ERROR: &str = "Processes can't send messages to themselves, \
    as this could result in deadlocks";

#[inline(always)]
pub(crate) fn allocate(state: &State, class_idx: u32) -> Pointer {
    let class_index = ClassIndex::new(class_idx);
    let class = unsafe { state.permanent_space.get_class(class_index) };
    let process = Process::alloc(class);

    process.as_pointer()
}

#[inline(always)]
pub(crate) fn send_message(
    state: &State,
    thread: &mut Thread,
    mut task: TaskPointer,
    sender: ProcessPointer,
    receiver_ptr: Pointer,
    method: u16,
    wait: bool,
) -> Result<bool, String> {
    let mut receiver = unsafe { ProcessPointer::from_pointer(receiver_ptr) };

    if sender == receiver {
        return Err(SEND_ERROR.to_string());
    }

    let args = task.take_arguments();

    match receiver.send_message(MethodIndex::new(method), sender, args, wait) {
        RescheduleRights::AcquiredWithTimeout => {
            state.timeout_worker.increase_expired_timeouts();
        }
        RescheduleRights::Acquired => {}
        _ => return Ok(false),
    }

    let switch = if wait {
        // When awaiting the result immediately we want to keep latency as small
        // as possible. To achieve this we reschedule the receiver (if allowed)
        // onto the current worker with a high priority.
        thread.schedule_priority(receiver);
        true
    } else {
        thread.schedule(receiver);
        false
    };

    Ok(switch)
}

#[inline(always)]
pub(crate) fn send_async_message(
    state: &State,
    thread: &mut Thread,
    sender: ProcessPointer,
    mut task: TaskPointer,
    receiver_ptr: Pointer,
    method: u16,
) -> Result<Pointer, String> {
    let mut receiver = unsafe { ProcessPointer::from_pointer(receiver_ptr) };

    if sender == receiver {
        return Err(SEND_ERROR.to_string());
    }

    let fut_state = FutureState::new();
    let fut =
        Future::alloc(state.permanent_space.future_class(), fut_state.clone());
    let args = task.take_arguments();

    match receiver.send_async_message(MethodIndex::new(method), fut_state, args)
    {
        RescheduleRights::AcquiredWithTimeout => {
            state.timeout_worker.increase_expired_timeouts();
            thread.schedule(receiver);
        }
        RescheduleRights::Acquired => {
            thread.schedule(receiver);
        }
        _ => {}
    }

    Ok(fut)
}

#[inline(always)]
pub(crate) fn suspend(
    state: &State,
    mut process: ProcessPointer,
    time_ptr: Pointer,
) {
    let nanos = unsafe { Int::read_u64(time_ptr) };
    let timeout = Timeout::with_rc(Duration::from_nanos(nanos));

    process.suspend(timeout.clone());
    state.timeout_worker.suspend(process, timeout);
}

#[inline(always)]
pub(crate) fn get_field(process: Pointer, index: u16) -> Pointer {
    unsafe { process.get::<Process>().get_field(FieldIndex::new(index as u8)) }
}

#[inline(always)]
pub(crate) fn set_field(process: Pointer, index: u16, value: Pointer) {
    unsafe {
        process
            .get_mut::<Process>()
            .set_field(FieldIndex::new(index as u8), value)
    }
}

#[inline(always)]
pub(crate) fn write_result(
    state: &State,
    thread: &mut Thread,
    task: TaskPointer,
    result: Pointer,
    thrown: bool,
) -> Pointer {
    match &task.write {
        Write::Discard => Pointer::false_singleton(),
        Write::Direct(mut rec) => {
            if thrown {
                rec.set_throw_value(result);
            } else {
                rec.set_return_value(result);
            }

            thread.schedule(rec);
            Pointer::true_singleton()
        }
        Write::Future(fut) => match fut.write(result, thrown) {
            WriteResult::Continue => Pointer::true_singleton(),
            WriteResult::Reschedule(consumer) => {
                thread.schedule(consumer);
                Pointer::true_singleton()
            }
            WriteResult::RescheduleWithTimeout(consumer) => {
                state.timeout_worker.increase_expired_timeouts();
                thread.schedule(consumer);
                Pointer::true_singleton()
            }
            WriteResult::Discard => Pointer::false_singleton(),
        },
    }
}
