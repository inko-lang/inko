use crate::context;
use crate::mem::{Array, ClassPointer, Int, Nil, String as InkoString};
use crate::process::{
    Channel, Message, NativeAsyncMethod, OwnedMessage, Process, ProcessPointer,
    ReceiveResult, RescheduleRights, SendResult, StackFrame,
};
use crate::result::Result as InkoResult;
use crate::scheduler::process::Action;
use crate::scheduler::timeouts::Timeout;
use crate::state::State;
use std::cmp::max;
use std::fmt::Write as _;
use std::process::exit;
use std::str;
use std::time::Duration;

const SEND_ERROR: &str = "Processes can't send messages to themselves, \
    as this could result in deadlocks";

/// Terminates the current program with an Inko panic (opposed to a panic
/// triggered using the `panic!` macro).
///
/// This function is marked as cold as we expect it to be called rarely, if ever
/// (in a correct program). This should also ensure any branches leading to this
/// function are treated as unlikely.
#[inline(never)]
#[cold]
pub(crate) fn panic(process: ProcessPointer, message: &str) -> ! {
    let mut buffer = String::new();

    buffer.push_str("Stack trace (the most recent call comes last):");

    for &frame in &process.call_stack {
        unsafe {
            let frame = &*frame;
            let _ = write!(
                buffer,
                "\n  {} line {}, in '{}'",
                InkoString::read(frame.path),
                Int::read(frame.line),
                InkoString::read(frame.name),
            );
        }
    }

    let _ = write!(
        buffer,
        "\nProcess {:#x} panicked: {}",
        process.identifier(),
        message
    );

    eprintln!("{}", buffer);
    exit(1);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_panic(
    process: ProcessPointer,
    message: *const InkoString,
) {
    let msg = &(*message).value;

    panic(process, msg);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_new(
    mut process: ProcessPointer,
    class: ClassPointer,
) -> ProcessPointer {
    let stack = process.thread().stacks.alloc();

    Process::alloc(class, stack)
}

#[no_mangle]
pub unsafe extern "system" fn inko_message_new(
    method: NativeAsyncMethod,
    length: u8,
) -> *mut Message {
    Message::new(method, length).into_raw()
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_send_message(
    state: *const State,
    mut sender: ProcessPointer,
    mut receiver: ProcessPointer,
    message: *mut Message,
) {
    if sender == receiver {
        panic(sender, SEND_ERROR);
    }

    let message = OwnedMessage::from_raw(message);
    let state = &*state;
    let reschedule = match receiver.send_message(message) {
        RescheduleRights::AcquiredWithTimeout => {
            state.timeout_worker.increase_expired_timeouts();
            true
        }
        RescheduleRights::Acquired => true,
        RescheduleRights::Failed => false,
    };

    if reschedule {
        sender.thread().schedule(receiver);
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_finish_message(
    mut process: ProcessPointer,
    terminate: bool,
) {
    let resched = process.finish_message();

    if terminate {
        // Safety: we can't terminate the process here as that would result in
        // us corrupting the current stack (= the process' stack), so instead we
        // defer this until we switch back to the thread's stack.
        process.thread().action = Action::Terminate;
    } else if resched {
        process.thread().schedule(process);
    }

    context::switch(process);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_yield(mut process: ProcessPointer) {
    // Safety: the current thread is holding on to the run lock
    process.thread().schedule(process);
    context::switch(process);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_suspend(
    state: *const State,
    mut process: ProcessPointer,
    nanos: i64,
) -> *const Nil {
    let timeout = Timeout::with_rc(Duration::from_nanos(nanos as _));
    let state = &*state;

    // Safety: the current thread is holding on to the run lock
    process.suspend(timeout.clone());
    state.timeout_worker.suspend(process, timeout);
    context::switch(process);
    state.nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_push_stack_frame(
    mut process: ProcessPointer,
    frame: *const StackFrame,
) {
    process.call_stack.push(frame);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_pop_stack_frame(
    mut process: ProcessPointer,
) {
    process.call_stack.pop();
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stack_frame_name(
    process: ProcessPointer,
    index: usize,
) -> *const InkoString {
    (*process.call_stack[index]).name
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stack_frame_path(
    process: ProcessPointer,
    index: usize,
) -> *const InkoString {
    (*process.call_stack[index]).path
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stack_frame_line(
    process: ProcessPointer,
    index: usize,
) -> *const Int {
    (*process.call_stack[index]).line
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stacktrace_length(
    state: *const State,
    process: ProcessPointer,
) -> *const Int {
    Int::new((*state).int_class, process.call_stack.len() as i64)
}

#[no_mangle]
pub unsafe extern "system" fn inko_channel_new(
    state: *const State,
    capacity: i64,
) -> *mut Channel {
    Channel::alloc((*state).channel_class, max(capacity, 1) as usize)
}

#[no_mangle]
pub unsafe extern "system" fn inko_channel_send(
    state: *const State,
    mut process: ProcessPointer,
    channel: *const Channel,
    message: *mut u8,
) -> *const Nil {
    let state = &*state;

    loop {
        match (*channel).send(process, message) {
            SendResult::Sent => break,
            SendResult::Full => context::switch(process),
            SendResult::Reschedule(receiver) => {
                process.thread().schedule_global(receiver);
                break;
            }
            SendResult::RescheduleWithTimeout(receiver) => {
                state.timeout_worker.increase_expired_timeouts();
                process.thread().schedule_global(receiver);
                break;
            }
        }
    }

    state.nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_channel_receive(
    mut process: ProcessPointer,
    channel: *const Channel,
) -> *const u8 {
    loop {
        match (*channel).receive(process, None) {
            ReceiveResult::None => context::switch(process),
            ReceiveResult::Some(msg) => return msg,
            ReceiveResult::Reschedule(msg, sender) => {
                // We schedule onto the global queue because the current process
                // wants to do something with the message.
                process.thread().schedule_global(sender);
                return msg;
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_channel_try_receive(
    mut process: ProcessPointer,
    channel: *const Channel,
) -> InkoResult {
    match (*channel).receive(process, None) {
        ReceiveResult::None => InkoResult::None,
        ReceiveResult::Some(msg) => InkoResult::Ok(msg),
        ReceiveResult::Reschedule(msg, sender) => {
            // We schedule onto the global queue because the current process
            // wants to do something with the message.
            process.thread().schedule_global(sender);
            InkoResult::Ok(msg)
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_channel_receive_until(
    state: *const State,
    mut process: ProcessPointer,
    channel: *const Channel,
    nanos: u64,
) -> InkoResult {
    let state = &(*state);
    let deadline = Timeout::from_nanos_deadline(state, nanos);

    loop {
        match (*channel).receive(process, Some(deadline.clone())) {
            ReceiveResult::None => {
                // Safety: the current thread is holding on to the run lock
                state.timeout_worker.suspend(process, deadline.clone());
                context::switch(process);

                if process.timeout_expired() {
                    state.timeout_worker.increase_expired_timeouts();
                    return InkoResult::None;
                }

                // It's possible another process received all messages before we
                // got a chance to try again. In this case we continue waiting
                // for a message.
            }
            ReceiveResult::Some(msg) => return InkoResult::Ok(msg),
            ReceiveResult::Reschedule(msg, sender) => {
                // We schedule onto the global queue because the current process
                // wants to do something with the message.
                process.thread().schedule_global(sender);
                return InkoResult::Ok(msg);
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_channel_drop(
    state: *const State,
    channel: *mut Channel,
) -> *const Nil {
    Channel::drop(channel);
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_channel_wait(
    state: *const State,
    process: ProcessPointer,
    channels: *mut Array,
) -> *const Nil {
    let channels = &mut *channels;
    let mut guards = Vec::with_capacity(channels.value.len());

    for &ptr in &channels.value {
        let chan = &(*(ptr as *const Channel));
        let guard = chan.state.lock().unwrap();

        if guard.has_messages() {
            return (*state).nil_singleton;
        }

        guards.push(guard);
    }

    // We have to hold on to the process state lock until all channels are
    // updated. If we don't do this, a process may write to a channel before
    // observing that we want to wait for messages, thus never rescheduling our
    // process.
    let mut proc_state = process.state();

    for mut guard in guards {
        guard.add_waiting_for_message(process);
    }

    proc_state.waiting_for_channel(None);
    drop(proc_state);

    // Safety: the current thread is holding on to the run lock, so a process
    // writing to one of the above channels can't reschedule us until the thread
    // releases the lock.
    context::switch(process);

    for &ptr in &channels.value {
        let chan = &(*(ptr as *const Channel));

        chan.state.lock().unwrap().remove_waiting_for_message(process);
    }

    (*state).nil_singleton
}
