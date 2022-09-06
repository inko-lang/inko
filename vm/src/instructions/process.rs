//! VM functions for working with Inko processes.
use crate::indexes::{ClassIndex, FieldIndex, MethodIndex};
use crate::mem::Float;
use crate::mem::Pointer;
use crate::process::{
    Finished, Future, FutureState, Process, ProcessPointer, RescheduleRights,
    TaskPointer, Write, WriteResult,
};
use crate::scheduler::timeouts::Timeout;
use crate::state::State;
use std::time::Duration;

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
    mut task: TaskPointer,
    sender: ProcessPointer,
    receiver_ptr: Pointer,
    method: u16,
    wait: bool,
) {
    let mut receiver = unsafe { ProcessPointer::from_pointer(receiver_ptr) };
    let args = task.take_arguments();

    match receiver.send_message(MethodIndex::new(method), sender, args, wait) {
        RescheduleRights::AcquiredWithTimeout => {
            state.timeout_worker.increase_expired_timeouts();
            state.scheduler.schedule(receiver);
        }
        RescheduleRights::Acquired => {
            state.scheduler.schedule(receiver);
        }
        _ => {}
    }
}

#[inline(always)]
pub(crate) fn send_async_message(
    state: &State,
    mut task: TaskPointer,
    receiver_ptr: Pointer,
    method: u16,
) -> Pointer {
    let mut receiver = unsafe { ProcessPointer::from_pointer(receiver_ptr) };
    let fut_state = FutureState::new();
    let fut =
        Future::alloc(state.permanent_space.future_class(), fut_state.clone());
    let args = task.take_arguments();

    match receiver.send_async_message(MethodIndex::new(method), fut_state, args)
    {
        RescheduleRights::AcquiredWithTimeout => {
            state.timeout_worker.increase_expired_timeouts();
            state.scheduler.schedule(receiver);
        }
        RescheduleRights::Acquired => {
            state.scheduler.schedule(receiver);
        }
        _ => {}
    }

    fut
}

#[inline(always)]
pub(crate) fn suspend(
    state: &State,
    mut process: ProcessPointer,
    time_ptr: Pointer,
) {
    let time = unsafe { Float::read(time_ptr) };
    let timeout = Timeout::with_rc(Duration::from_secs_f64(time));

    process.suspend(timeout.clone());
    state.timeout_worker.suspend(process, timeout);
}

#[inline(always)]
pub(crate) fn finish_task(state: &State, mut process: ProcessPointer) {
    match process.finish_task() {
        Finished::Reschedule => state.scheduler.schedule(process),
        Finished::Terminate => {
            if process.is_main() {
                state.terminate();
            }

            // Processes drop/free themselves as this must be deferred until all
            // messages (including any destructors) have finished running. If we
            // did this in a destructor we'd end up releasing memory of a
            // process while still using it.
            Process::drop_and_deallocate(process);
        }
        Finished::WaitForMessage => {
            // When waiting for a message, clients will reschedule or terminate
            // the process when needed. This means at this point we can't use
            // the process anymore, as it may have already been rescheduled.
        }
    }
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

            state.scheduler.schedule(rec);
            Pointer::true_singleton()
        }
        Write::Future(fut) => match fut.write(result, thrown) {
            WriteResult::Continue => Pointer::true_singleton(),
            WriteResult::Reschedule(consumer) => {
                state.scheduler.schedule(consumer);
                Pointer::true_singleton()
            }
            WriteResult::RescheduleWithTimeout(consumer) => {
                state.timeout_worker.increase_expired_timeouts();
                state.scheduler.schedule(consumer);
                Pointer::true_singleton()
            }
            WriteResult::Discard => Pointer::false_singleton(),
        },
    }
}
