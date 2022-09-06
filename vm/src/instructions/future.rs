//! VM functions for working with Inko futures.
use crate::mem::{Array, Float, Pointer};
use crate::process::{Future, FutureResult, ProcessPointer};
use crate::scheduler::timeouts::Timeout;
use crate::state::State;
use std::time::Duration;

#[inline(always)]
pub(crate) fn get(
    _: &State,
    mut process: ProcessPointer,
    future_ptr: Pointer,
) -> bool {
    let fut = unsafe { future_ptr.get::<Future>() };

    match fut.get(process, None) {
        FutureResult::Returned(val) => {
            process.set_return_value(val);
            true
        }
        FutureResult::Thrown(val) => {
            process.set_throw_value(val);
            true
        }
        FutureResult::None => false,
    }
}

#[inline(always)]
pub(crate) fn get_for(
    state: &State,
    mut process: ProcessPointer,
    future_ptr: Pointer,
    time_ptr: Pointer,
) -> bool {
    if process.timeout_expired() {
        state.timeout_worker.increase_expired_timeouts();
        process.set_return_value(Pointer::undefined_singleton());

        return true;
    }

    let timeout = Timeout::with_rc(Duration::from_secs_f64(unsafe {
        Float::read(time_ptr)
    }));
    let fut_ref = unsafe { future_ptr.get::<Future>() };

    match fut_ref.get(process, Some(timeout.clone())) {
        FutureResult::Returned(val) => {
            process.set_return_value(val);
            true
        }
        FutureResult::Thrown(val) => {
            process.set_throw_value(val);
            true
        }
        FutureResult::None => {
            state.timeout_worker.suspend(process, timeout);
            false
        }
    }
}

#[inline(always)]
pub(crate) fn drop(pointer: Pointer) -> Pointer {
    unsafe {
        let result = pointer.get::<Future>().disconnect();

        Future::drop(pointer);
        result
    }
}

#[inline(always)]
pub(crate) fn poll(
    state: &State,
    process: ProcessPointer,
    pending_ptr: Pointer,
) -> Option<Pointer> {
    let pending = unsafe { pending_ptr.get_mut::<Array>() };
    let mut ready = Vec::new();

    if pending.value().is_empty() {
        // If the input is already empty we can just skip all the work below.
        // This way polling an empty Array won't result in the process hanging
        // forever.
        return Some(Array::alloc(state.permanent_space.array_class(), ready));
    }

    let mut locks: Vec<_> = pending
        .value()
        .iter()
        .map(|p| (*p, unsafe { p.get::<Future>() }.lock()))
        .collect();

    pending.value_mut().clear();

    for (p, lock) in &mut locks {
        // We _must_ ensure the consumer is cleared. If we don't, the following
        // could happen:
        //
        // 1. We return a bunch of ready futures, but a few are not ready yet.
        // 2. We wait for an unrelated future.
        // 3. One of the futures is now ready and reschedules us.
        // 4. All hell breaks loose.
        lock.consumer = None;

        let target =
            if lock.has_result() { &mut ready } else { pending.value_mut() };

        target.push(*p);
    }

    if ready.is_empty() {
        process.state().waiting_for_future(None);

        for (_, lock) in &mut locks {
            lock.consumer = Some(process);
        }

        None
    } else {
        Some(Array::alloc(state.permanent_space.array_class(), ready))
    }
}
