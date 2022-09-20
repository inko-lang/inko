use crate::arc_without_weak::ArcWithoutWeak;
use crate::execution_context::ExecutionContext;
use crate::indexes::{FieldIndex, MethodIndex};
use crate::location_table::Location;
use crate::mem::{allocate, ClassPointer, Header, MethodPointer, Pointer};
use crate::scheduler::timeouts::Timeout;
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::collections::VecDeque;
use std::mem::{align_of, size_of, swap};
use std::ops::Drop;
use std::ops::{Deref, DerefMut};
use std::ptr::{copy_nonoverlapping, drop_in_place, NonNull};
use std::slice;
use std::sync::{Mutex, MutexGuard};

/// The shared state of a future.
pub(crate) struct FutureState {
    /// The process that's waiting for this future to complete.
    pub(crate) consumer: Option<ProcessPointer>,

    /// The result of a message.
    ///
    /// This defaults to the undefined singleton.
    result: Pointer,

    /// A boolean indicating that the result was produced by throwing an error,
    /// instead of returning it.
    thrown: bool,

    /// A boolean indicating the future is disconnected.
    ///
    /// This flag is set whenever a reader or writer is dropped.
    disconnected: bool,
}

impl FutureState {
    pub(crate) fn new() -> RcFutureState {
        let state = Self {
            consumer: None,
            result: Pointer::undefined_singleton(),
            thrown: false,
            disconnected: false,
        };

        ArcWithoutWeak::new(Mutex::new(state))
    }

    pub(crate) fn has_result(&self) -> bool {
        self.result != Pointer::undefined_singleton()
    }
}

type RcFutureState = ArcWithoutWeak<Mutex<FutureState>>;

impl RcFutureState {
    /// Writes the result of a message to this future.
    ///
    /// If the future has been disconnected, an Err is returned that contains
    /// the value that we tried to write.
    ///
    /// The returned tuple contains a value that indicates if a process needs to
    /// be rescheduled, and a pointer to the process that sent the message.
    pub(crate) fn write(&self, result: Pointer, thrown: bool) -> WriteResult {
        let mut future = self.lock().unwrap();

        if future.disconnected {
            return WriteResult::Discard;
        }

        future.thrown = thrown;
        future.result = result;

        if let Some(consumer) = future.consumer.take() {
            match consumer.state.lock().unwrap().try_reschedule_for_future() {
                RescheduleRights::Failed => WriteResult::Continue,
                RescheduleRights::Acquired => WriteResult::Reschedule(consumer),
                RescheduleRights::AcquiredWithTimeout => {
                    WriteResult::RescheduleWithTimeout(consumer)
                }
            }
        } else {
            WriteResult::Continue
        }
    }

    /// Gets the value from this future.
    ///
    /// When a None is returned, the consumer must stop running any code. It's
    /// then up to a producer to reschedule the consumer when writing a value to
    /// the future.
    pub(crate) fn get(
        &self,
        consumer: ProcessPointer,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) -> FutureResult {
        // The locking order is important here: we _must_ lock the future
        // _before_ locking the consumer. If we lock the consumer first, we
        // could deadlock with processes writing to this future.
        let mut future = self.lock().unwrap();
        let mut state = consumer.state.lock().unwrap();
        let result = future.result;

        if result != Pointer::undefined_singleton() {
            future.consumer = None;
            future.result = Pointer::undefined_singleton();

            state.status.set_waiting_for_future(false);

            return if future.thrown {
                FutureResult::Thrown(result)
            } else {
                FutureResult::Returned(result)
            };
        }

        future.consumer = Some(consumer);

        state.waiting_for_future(timeout);
        FutureResult::None
    }
}

/// A method scheduled for execution at some point in the future.
///
/// The size of this type depends on the number of arguments. Using this
/// approach allows us to keep the size of a message as small as possible,
/// without the need for allocating arguments separately.
#[repr(C)]
struct ScheduledMethodInner {
    /// The method to run.
    method: MethodIndex,

    /// The number of arguments of this message.
    length: u8,

    /// The arguments of the message.
    arguments: [Pointer; 0],
}

/// An owned pointer to a ScheduledMethodInner.
struct ScheduledMethod(NonNull<ScheduledMethodInner>);

impl ScheduledMethod {
    fn new(method: MethodIndex, arguments: Vec<Pointer>) -> Self {
        unsafe {
            let layout = Self::layout(arguments.len() as u8);
            let raw_ptr = alloc(layout) as *mut ScheduledMethodInner;

            if raw_ptr.is_null() {
                handle_alloc_error(layout);
            }

            let msg = &mut *raw_ptr;

            init!(msg.method => method);
            init!(msg.length => arguments.len() as u8);

            copy_nonoverlapping(
                arguments.as_ptr(),
                msg.arguments.as_mut_ptr(),
                arguments.len(),
            );

            Self(NonNull::new_unchecked(raw_ptr))
        }
    }

    unsafe fn layout(length: u8) -> Layout {
        let size = size_of::<ScheduledMethodInner>()
            + (length as usize * size_of::<Pointer>());

        // Messages are sent often, so we don't want the overhead of size and
        // alignment checks.
        Layout::from_size_align_unchecked(size, align_of::<Self>())
    }
}

impl Deref for ScheduledMethod {
    type Target = ScheduledMethodInner;

    fn deref(&self) -> &ScheduledMethodInner {
        unsafe { self.0.as_ref() }
    }
}

impl Drop for ScheduledMethod {
    fn drop(&mut self) {
        unsafe {
            let layout = Self::layout(self.0.as_ref().length);

            drop_in_place(self.0.as_ptr());
            dealloc(self.0.as_ptr() as *mut u8, layout);
        }
    }
}

/// A message sent between two processes.
struct Message {
    write: Write,
    scheduled: ScheduledMethod,
}

impl Message {
    fn new(
        method: MethodIndex,
        write: Write,
        arguments: Vec<Pointer>,
    ) -> Message {
        let scheduled = ScheduledMethod::new(method, arguments);

        Message { write, scheduled }
    }
}

/// A collection of messages to be processed by a process.
struct Mailbox {
    messages: VecDeque<Message>,
}

impl Mailbox {
    fn new() -> Self {
        Mailbox { messages: VecDeque::new() }
    }

    fn send(&mut self, message: Message) {
        self.messages.push_back(message);
    }

    fn receive(&mut self) -> Option<Message> {
        self.messages.pop_front()
    }
}

/// A type indicating how the results of a task should be communicated with the
/// consumer.
pub(crate) enum Write {
    /// The result of a task is to be discarded.
    Discard,

    /// The consumer/sender is suspended and waiting for a result, without using
    /// a future.
    Direct(ProcessPointer),

    /// The consumer scheduled the message without immediately waiting for it.
    /// Instead of writing the result directly to the consumer, we write it to a
    /// future.
    Future(RcFutureState),
}

/// A task to run in response to a message.
pub(crate) struct Task {
    /// The execution context/call stack of this task.
    pub(crate) context: Box<ExecutionContext>,

    /// The stack of arguments for the next instruction.
    ///
    /// Certain instructions use a variable number of arguments, such as method
    /// calls. For these instructions we use a stack, as the VM's instructions
    /// are of a fixed size.
    ///
    /// The ordering of values may differ per instruction: for method calls
    /// arguments should be pushed in reverse order, so the first pop() returns
    /// the first argument instead of the last one. For other instructions the
    /// values are in-order, meaning the first value in the stack is the first
    /// value to use.
    ///
    /// In the past we used to put registers next to each other, and specified
    /// the first register and the number of arguments. The VM would then read
    /// all arguments in sequence. While this removes the need for a stack, it
    /// complicated code generation enough that we decided to move away from it.
    pub(crate) stack: Vec<Pointer>,
    pub(crate) write: Write,
}

impl Task {
    pub(crate) fn new(context: ExecutionContext, write: Write) -> Self {
        Self { context: Box::new(context), write, stack: Vec::new() }
    }

    pub(crate) fn take_arguments(&mut self) -> Vec<Pointer> {
        let mut stack = Vec::new();

        swap(&mut stack, &mut self.stack);
        stack
    }

    pub(crate) fn clear_arguments(&mut self) {
        self.stack.clear();
    }

    /// Adds a new execution context to the stack.
    ///
    /// The parent of the new context is set to the context before the push.
    pub(crate) fn push_context(&mut self, new_context: ExecutionContext) {
        let mut boxed = Box::new(new_context);
        let target = &mut self.context;

        swap(target, &mut boxed);
        target.parent = Some(boxed);
    }

    /// Pops the current execution context off the stack.
    ///
    /// If all contexts have been popped, `true` is returned.
    pub(crate) fn pop_context(&mut self) -> bool {
        let context = &mut self.context;

        if let Some(parent) = context.parent.take() {
            *context = parent;
            false
        } else {
            true
        }
    }

    pub(crate) fn contexts(&self) -> Vec<&ExecutionContext> {
        self.context.contexts().collect::<Vec<_>>()
    }
}

/// A pointer to a Task.
///
/// In various places we borrow a Task while also borrowing its fields, or while
/// borrowing fields of the owning Process. This can't be expressed safely in
/// Rust's type system, and working around this requires fiddling with pointer
/// casts and unsafe code.
///
/// To make this less painful to deal with, instead of using a `&mut Task` in
/// various places we use a `TaskPointer`. This won't give us any extra safety,
/// but at least it's less annoying to deal with.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct TaskPointer(Pointer);

impl TaskPointer {
    fn new(pointer: &Task) -> Self {
        Self(Pointer::new(pointer as *const _ as *mut _))
    }
}

impl Deref for TaskPointer {
    type Target = Task;

    fn deref(&self) -> &Task {
        unsafe { &*(self.0.as_ptr() as *mut Task) }
    }
}

impl DerefMut for TaskPointer {
    fn deref_mut(&mut self) -> &mut Task {
        unsafe { &mut *(self.0.as_ptr() as *mut Task) }
    }
}

/// The status of a process, represented as a set of bits.
pub(crate) struct ProcessStatus {
    /// The bits used to indicate the status of the process.
    ///
    /// Multiple bits may be set in order to combine different statuses.
    bits: u8,
}

impl ProcessStatus {
    /// A regular process.
    const NORMAL: u8 = 0b00_0000;

    /// The main process.
    const MAIN: u8 = 0b00_0001;

    /// The process is waiting for a message.
    const WAITING_FOR_MESSAGE: u8 = 0b00_0010;

    /// The process is waiting for a future.
    const WAITING_FOR_FUTURE: u8 = 0b00_0100;

    /// The process is simply sleeping for a certain amount of time.
    const SLEEPING: u8 = 0b00_1000;

    /// The process was rescheduled after a timeout expired.
    const TIMEOUT_EXPIRED: u8 = 0b01_0000;

    /// The process is waiting for something, or suspended for a period of time
    const WAITING: u8 = Self::WAITING_FOR_FUTURE | Self::SLEEPING;

    pub(crate) fn new() -> Self {
        Self { bits: Self::NORMAL }
    }

    fn set_main(&mut self) {
        self.update_bits(Self::MAIN, true);
    }

    fn is_main(&self) -> bool {
        self.bit_is_set(Self::MAIN)
    }

    fn set_waiting_for_message(&mut self, enable: bool) {
        self.update_bits(Self::WAITING_FOR_MESSAGE, enable);
    }

    fn is_waiting_for_message(&self) -> bool {
        self.bit_is_set(Self::WAITING_FOR_MESSAGE)
    }

    fn set_waiting_for_future(&mut self, enable: bool) {
        self.update_bits(Self::WAITING_FOR_FUTURE, enable);
    }

    fn is_waiting_for_future(&self) -> bool {
        self.bit_is_set(Self::WAITING_FOR_FUTURE)
    }

    fn is_waiting(&self) -> bool {
        (self.bits & Self::WAITING) != 0
    }

    fn no_longer_waiting(&mut self) {
        self.update_bits(Self::WAITING, false);
    }

    fn set_timeout_expired(&mut self, enable: bool) {
        self.update_bits(Self::TIMEOUT_EXPIRED, enable)
    }

    fn set_sleeping(&mut self, enable: bool) {
        self.update_bits(Self::SLEEPING, enable);
    }

    fn timeout_expired(&self) -> bool {
        self.bit_is_set(Self::TIMEOUT_EXPIRED)
    }

    fn update_bits(&mut self, mask: u8, enable: bool) {
        self.bits = if enable { self.bits | mask } else { self.bits & !mask };
    }

    fn bit_is_set(&self, bit: u8) -> bool {
        self.bits & bit == bit
    }
}

/// An enum describing what rights a thread was given when trying to reschedule
/// a process.
#[derive(Eq, PartialEq, Debug)]
pub(crate) enum RescheduleRights {
    /// The rescheduling rights were not obtained.
    Failed,

    /// The rescheduling rights were obtained.
    Acquired,

    /// The rescheduling rights were obtained, and the process was using a
    /// timeout.
    AcquiredWithTimeout,
}

impl RescheduleRights {
    pub(crate) fn are_acquired(&self) -> bool {
        !matches!(self, RescheduleRights::Failed)
    }
}

/// The shared state of a process.
///
/// This state is shared by both the process and its clients.
pub(crate) struct ProcessState {
    /// The mailbox of this process.
    mailbox: Mailbox,

    /// The status of the process.
    status: ProcessStatus,

    /// The timeout this process is suspended with, if any.
    ///
    /// If missing and the process is suspended, it means the process is
    /// suspended indefinitely.
    timeout: Option<ArcWithoutWeak<Timeout>>,
}

impl ProcessState {
    pub(crate) fn new() -> Self {
        Self {
            mailbox: Mailbox::new(),
            status: ProcessStatus::new(),
            timeout: None,
        }
    }

    pub(crate) fn has_same_timeout(
        &self,
        timeout: &ArcWithoutWeak<Timeout>,
    ) -> bool {
        self.timeout
            .as_ref()
            .map(|t| t.as_ptr() == timeout.as_ptr())
            .unwrap_or(false)
    }

    pub(crate) fn try_reschedule_after_timeout(&mut self) -> RescheduleRights {
        if !self.status.is_waiting() {
            return RescheduleRights::Failed;
        }

        if self.status.is_waiting_for_future() {
            // If we were waiting for a future, it means the timeout has
            // expired.
            self.status.set_timeout_expired(true);
        }

        self.status.no_longer_waiting();

        if self.timeout.take().is_some() {
            RescheduleRights::AcquiredWithTimeout
        } else {
            RescheduleRights::Acquired
        }
    }

    pub(crate) fn waiting_for_future(
        &mut self,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) {
        self.timeout = timeout;

        self.status.set_waiting_for_future(true);
    }

    fn try_reschedule_for_message(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_message() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_message(false);
        RescheduleRights::Acquired
    }

    fn try_reschedule_for_future(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_future() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_future(false);

        if self.timeout.take().is_some() {
            RescheduleRights::AcquiredWithTimeout
        } else {
            RescheduleRights::Acquired
        }
    }
}

/// A lightweight process.
#[repr(C)]
pub(crate) struct Process {
    pub(crate) header: Header,

    /// A boolean indicating that the result was thrown rather than returned.
    thrown: bool,

    /// The currently running task, if any.
    task: Option<Task>,

    /// The last value returned or thrown.
    result: Pointer,

    /// The internal shared state of the process.
    state: Mutex<ProcessState>,

    /// The fields of this process.
    ///
    /// The length of this flexible array is derived from the number of
    /// fields defined in this process' class.
    fields: [Pointer; 0],
}

impl Process {
    pub(crate) fn drop_and_deallocate(ptr: ProcessPointer) {
        unsafe {
            drop_in_place(ptr.as_pointer().as_ptr() as *mut Self);
            ptr.as_pointer().free();
        }
    }

    pub(crate) fn alloc(class: ClassPointer) -> ProcessPointer {
        let ptr = allocate(unsafe { class.instance_layout() });
        let obj = unsafe { ptr.get_mut::<Self>() };
        let mut state = ProcessState::new();

        // Processes start without any messages, so we must ensure their status
        // is set accordingly.
        state.status.set_waiting_for_message(true);

        obj.header.init_atomic(class);

        init!(obj.thrown => false);
        init!(obj.result => Pointer::undefined_singleton());
        init!(obj.state => Mutex::new(state));
        init!(obj.task => None);

        unsafe { ProcessPointer::from_pointer(ptr) }
    }

    /// Returns a new Process acting as the main process.
    ///
    /// This process always runs on the main thread.
    pub(crate) fn main(
        class: ClassPointer,
        method: MethodPointer,
    ) -> ProcessPointer {
        let mut process = Self::alloc(class);
        let mut task = Task::new(ExecutionContext::new(method), Write::Discard);

        task.stack.push(process.as_pointer());

        process.task = Some(task);

        // The main process always has an initial message, so we need to reset
        // this particular status.
        process.state().status.set_waiting_for_message(false);
        process.set_main();
        process
    }

    pub(crate) fn set_main(&mut self) {
        self.state.lock().unwrap().status.set_main();
    }

    pub(crate) fn is_main(&self) -> bool {
        self.state.lock().unwrap().status.is_main()
    }

    /// Suspends this process for a period of time.
    ///
    /// A process is sleeping when it simply isn't to be scheduled for a while,
    /// without waiting for a message, future, or something else.
    pub(crate) fn suspend(&mut self, timeout: ArcWithoutWeak<Timeout>) {
        let mut state = self.state.lock().unwrap();

        state.timeout = Some(timeout);

        state.status.set_sleeping(true);
    }

    /// Sends a synchronous message to this process.
    pub(crate) fn send_message(
        &mut self,
        method: MethodIndex,
        sender: ProcessPointer,
        arguments: Vec<Pointer>,
        wait: bool,
    ) -> RescheduleRights {
        let write = if wait { Write::Direct(sender) } else { Write::Discard };
        let message = Message::new(method, write, arguments);
        let mut state = self.state.lock().unwrap();

        state.mailbox.send(message);
        state.try_reschedule_for_message()
    }

    /// Sends an asynchronous message to this process.
    pub(crate) fn send_async_message(
        &mut self,
        method: MethodIndex,
        future: RcFutureState,
        arguments: Vec<Pointer>,
    ) -> RescheduleRights {
        let message = Message::new(method, Write::Future(future), arguments);
        let mut state = self.state.lock().unwrap();

        state.mailbox.send(message);
        state.try_reschedule_for_message()
    }

    /// Schedules a task to run, if none is scheduled already.
    pub(crate) fn task_to_run(&mut self) -> Option<TaskPointer> {
        let mut proc_state = self.state.lock().unwrap();

        if let Some(task) = self.task.as_ref() {
            return Some(TaskPointer::new(task));
        }

        let message = if let Some(message) = proc_state.mailbox.receive() {
            message
        } else {
            proc_state.status.set_waiting_for_message(true);

            return None;
        };

        drop(proc_state);

        let method =
            unsafe { self.header.class.get_method(message.scheduled.method) };
        let ctx = ExecutionContext::new(method);
        let len = message.scheduled.length as usize;
        let values = unsafe {
            slice::from_raw_parts(message.scheduled.arguments.as_ptr(), len)
        };

        let mut task = Task::new(ctx, message.write);

        task.stack.extend_from_slice(values);

        self.task = Some(task);

        self.task.as_ref().map(TaskPointer::new)
    }

    /// Finishes the exection of a task, and decides what to do next with this
    /// process.
    ///
    /// If the return value is `true`, the process should be rescheduled.
    pub(crate) fn finish_task(&mut self) -> bool {
        let mut state = self.state.lock().unwrap();

        self.task.take();

        if state.mailbox.messages.is_empty() {
            state.status.set_waiting_for_message(true);
            false
        } else {
            true
        }
    }

    pub(crate) unsafe fn set_field(
        &mut self,
        index: FieldIndex,
        value: Pointer,
    ) {
        self.fields.as_mut_ptr().add(index.into()).write(value);
    }

    pub(crate) unsafe fn get_field(&self, index: FieldIndex) -> Pointer {
        *self.fields.as_ptr().add(index.into())
    }

    pub(crate) fn timeout_expired(&self) -> bool {
        let mut state = self.state.lock().unwrap();

        if state.status.timeout_expired() {
            state.status.set_timeout_expired(false);
            true
        } else {
            false
        }
    }

    pub(crate) fn state(&self) -> MutexGuard<ProcessState> {
        self.state.lock().unwrap()
    }

    pub(crate) fn stacktrace(&self) -> Vec<Location> {
        let mut locations = Vec::new();

        if let Some(task) = self.task.as_ref() {
            for context in task.contexts() {
                let mut index = context.index;
                let ins = &context.method.instructions[index];

                // When entering methods the index points to the instruction
                // _after_ the call. For built-in function calls this isn't the
                // case, as we store the current index instead so the call can
                // be retried (e.g. when a socket operation) would block.
                if index > 0 && !ins.opcode.rewind_before_call() {
                    index -= 1;
                }

                if let Some(loc) = context.method.locations.get(index as u32) {
                    locations.push(loc);
                }
            }

            // The most recent frame should come first, not last.
            locations.reverse();
        }

        locations
    }

    pub(crate) fn thrown(&self) -> bool {
        self.thrown
    }

    pub(crate) fn set_return_value(&mut self, value: Pointer) {
        self.result = value;
    }

    pub(crate) fn set_throw_value(&mut self, value: Pointer) {
        self.thrown = true;
        self.result = value;
    }

    pub(crate) fn move_result(&mut self) -> Pointer {
        let result = self.result;

        self.result = Pointer::undefined_singleton();
        self.thrown = false;

        result
    }
}

/// A pointer to a process.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct ProcessPointer(NonNull<Process>);

unsafe impl Sync for ProcessPointer {}
unsafe impl Send for ProcessPointer {}

impl ProcessPointer {
    pub(crate) unsafe fn from_pointer(pointer: Pointer) -> Self {
        Self::new(pointer.as_ptr() as *mut Process)
    }

    /// Returns a new pointer from a raw Pointer.
    pub(crate) unsafe fn new(pointer: *mut Process) -> Self {
        Self(NonNull::new_unchecked(pointer))
    }

    pub(crate) fn identifier(self) -> usize {
        self.0.as_ptr() as usize
    }

    pub(crate) fn as_pointer(self) -> Pointer {
        Pointer::new(self.0.as_ptr() as *mut u8)
    }
}

impl Deref for ProcessPointer {
    type Target = Process;

    fn deref(&self) -> &Process {
        unsafe { &*self.0.as_ptr() }
    }
}

impl DerefMut for ProcessPointer {
    fn deref_mut(&mut self) -> &mut Process {
        unsafe { &mut *self.0.as_mut() }
    }
}

/// An enum describing the value produced by a future, and how it was produced.
#[derive(PartialEq, Eq, Debug)]
pub(crate) enum FutureResult {
    /// No values has been produced so far.
    None,

    /// The value was returned.
    Returned(Pointer),

    /// The value was thrown and should be thrown again.
    Thrown(Pointer),
}

/// An enum that describes what a producer should do after writing to a future.
#[derive(PartialEq, Eq, Debug)]
pub(crate) enum WriteResult {
    /// The future is disconnected, and the writer should discard the value it
    /// tried to write.
    Discard,

    /// A value was written, but no further action is needed.
    Continue,

    /// A value was written, and the given process needs to be rescheduled.
    Reschedule(ProcessPointer),

    /// A value was written, the given process needs to be rescheduled, and a
    /// timeout needs to be invalidated.
    RescheduleWithTimeout(ProcessPointer),
}

/// Storage for a value to be computed some time in the future.
///
/// A Future is essentially just a single-producer single-consumer queue, with
/// support for only writing a value once, and only reading it once. Futures are
/// used to send message results between processes.
#[repr(C)]
pub(crate) struct Future {
    header: Header,
    state: RcFutureState,
}

impl Future {
    pub(crate) fn alloc(class: ClassPointer, state: RcFutureState) -> Pointer {
        let ptr = allocate(Layout::new::<Self>());
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);
        init!(obj.state => state);
        ptr
    }

    pub(crate) unsafe fn drop(ptr: Pointer) {
        drop_in_place(ptr.untagged_ptr() as *mut Self);
    }

    pub(crate) fn get(
        &self,
        consumer: ProcessPointer,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) -> FutureResult {
        self.state.get(consumer, timeout)
    }

    pub(crate) fn lock(&self) -> MutexGuard<FutureState> {
        self.state.lock().unwrap()
    }

    pub(crate) fn disconnect(&self) -> Pointer {
        let mut future = self.state.lock().unwrap();
        let result = future.result;

        future.disconnected = true;
        future.result = Pointer::undefined_singleton();

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::location_table::Location;
    use crate::mem::{Class, Method};
    use crate::test::{
        empty_class, empty_method, empty_process_class, new_process,
        OwnedClass, OwnedProcess,
    };
    use std::time::Duration;

    #[test]
    fn test_message_new() {
        let method = MethodIndex::new(0);
        let future = FutureState::new();
        let write = Write::Future(future);
        let message =
            Message::new(method, write, vec![Pointer::int(1), Pointer::int(2)]);

        assert_eq!(0_u16, message.scheduled.method.into());
        assert_eq!(message.scheduled.length, 2);

        unsafe {
            assert_eq!(
                *message.scheduled.arguments.as_ptr().add(0),
                Pointer::int(1)
            );
            assert_eq!(
                *message.scheduled.arguments.as_ptr().add(1),
                Pointer::int(2)
            );
        }
    }

    #[test]
    fn test_mailbox_send_receive() {
        let method = MethodIndex::new(0);
        let future = FutureState::new();
        let write = Write::Future(future);
        let message =
            Message::new(method, write, vec![Pointer::int(1), Pointer::int(2)]);
        let mut mailbox = Mailbox::new();

        mailbox.send(message);

        let message = mailbox.receive().unwrap();

        assert_eq!(message.scheduled.length, 2);
    }

    #[test]
    fn test_process_status_new() {
        let status = ProcessStatus::new();

        assert_eq!(status.bits, 0);
    }

    #[test]
    fn test_process_status_set_main() {
        let mut status = ProcessStatus::new();

        status.set_main();

        assert!(status.is_main());
    }

    #[test]
    fn test_process_status_set_waiting_for_message() {
        let mut status = ProcessStatus::new();

        status.set_waiting_for_message(true);
        assert!(status.is_waiting_for_message());

        status.set_waiting_for_message(false);
        assert!(!status.is_waiting_for_message());
    }

    #[test]
    fn test_process_status_set_waiting_for_future() {
        let mut status = ProcessStatus::new();

        status.set_waiting_for_future(true);
        assert!(status.is_waiting_for_future());

        status.set_waiting_for_future(false);
        assert!(!status.is_waiting_for_future());
    }

    #[test]
    fn test_process_status_is_waiting() {
        let mut status = ProcessStatus::new();

        status.set_sleeping(true);
        assert!(status.is_waiting());

        status.set_sleeping(false);
        status.set_waiting_for_future(true);
        assert!(status.is_waiting());

        status.no_longer_waiting();

        assert!(!status.is_waiting_for_future());
        assert!(!status.is_waiting());
    }

    #[test]
    fn test_process_status_timeout_expired() {
        let mut status = ProcessStatus::new();

        status.set_timeout_expired(true);
        assert!(status.timeout_expired());

        status.set_timeout_expired(false);
        assert!(!status.timeout_expired());
    }

    #[test]
    fn test_reschedule_rights_are_acquired() {
        assert!(!RescheduleRights::Failed.are_acquired());
        assert!(RescheduleRights::Acquired.are_acquired());
        assert!(RescheduleRights::AcquiredWithTimeout.are_acquired());
    }

    #[test]
    fn test_process_state_has_same_timeout() {
        let mut state = ProcessState::new();
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        assert!(!state.has_same_timeout(&timeout));

        state.timeout = Some(timeout.clone());

        assert!(state.has_same_timeout(&timeout));
    }

    #[test]
    fn test_process_state_try_reschedule_after_timeout() {
        let mut state = ProcessState::new();

        assert_eq!(
            state.try_reschedule_after_timeout(),
            RescheduleRights::Failed
        );

        state.waiting_for_future(None);

        assert_eq!(
            state.try_reschedule_after_timeout(),
            RescheduleRights::Acquired
        );

        assert!(!state.status.is_waiting_for_future());
        assert!(!state.status.is_waiting());

        let timeout = Timeout::with_rc(Duration::from_secs(0));

        state.waiting_for_future(Some(timeout));

        assert_eq!(
            state.try_reschedule_after_timeout(),
            RescheduleRights::AcquiredWithTimeout
        );

        assert!(!state.status.is_waiting_for_future());
        assert!(!state.status.is_waiting());
    }

    #[test]
    fn test_process_state_waiting_for_future() {
        let mut state = ProcessState::new();
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        state.waiting_for_future(None);

        assert!(state.status.is_waiting_for_future());
        assert!(state.timeout.is_none());

        state.waiting_for_future(Some(timeout));

        assert!(state.status.is_waiting_for_future());
        assert!(state.timeout.is_some());
    }

    #[test]
    fn test_process_state_try_reschedule_for_message() {
        let mut state = ProcessState::new();

        assert_eq!(
            state.try_reschedule_for_message(),
            RescheduleRights::Failed
        );

        state.status.set_waiting_for_message(true);

        assert_eq!(
            state.try_reschedule_for_message(),
            RescheduleRights::Acquired
        );
        assert!(!state.status.is_waiting_for_message());
    }

    #[test]
    fn test_process_state_try_reschedule_for_future() {
        let mut state = ProcessState::new();

        assert_eq!(state.try_reschedule_for_future(), RescheduleRights::Failed);

        state.status.set_waiting_for_future(true);
        assert_eq!(
            state.try_reschedule_for_future(),
            RescheduleRights::Acquired
        );
        assert!(!state.status.is_waiting_for_future());

        state.status.set_waiting_for_future(true);
        state.timeout = Some(Timeout::with_rc(Duration::from_secs(0)));

        assert_eq!(
            state.try_reschedule_for_future(),
            RescheduleRights::AcquiredWithTimeout
        );
        assert!(!state.status.is_waiting_for_future());
    }

    #[test]
    fn test_process_new() {
        let class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(*class));

        assert_eq!(process.header.class.as_pointer(), class.as_pointer());
        assert!(process.task.is_none());
    }

    #[test]
    fn test_process_main() {
        let proc_class = empty_process_class("A");
        let method = empty_method();
        let process = OwnedProcess::new(Process::main(*proc_class, method));

        assert!(process.is_main());

        Method::drop_and_deallocate(method);
    }

    #[test]
    fn test_process_set_main() {
        let class = empty_process_class("A");
        let mut process = OwnedProcess::new(Process::alloc(*class));

        assert!(!process.is_main());

        process.set_main();
        assert!(process.is_main());
    }

    #[test]
    fn test_process_suspend() {
        let class = empty_process_class("A");
        let mut process = OwnedProcess::new(Process::alloc(*class));
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        process.suspend(timeout);

        assert!(process.state().timeout.is_some());
        assert!(process.state().status.is_waiting());
    }

    #[test]
    fn test_process_send_message() {
        let proc_class = OwnedClass::new(Class::process("A".to_string(), 0, 1));
        let method = empty_method();
        let future = FutureState::new();
        let index = MethodIndex::new(0);

        unsafe { proc_class.set_method(index, method) };

        let mut process = OwnedProcess::new(Process::alloc(*proc_class));

        process.send_async_message(index, future, vec![Pointer::int(42)]);

        let mut state = process.state();

        assert_eq!(state.mailbox.messages.len(), 1);
        assert_eq!(state.mailbox.receive().unwrap().scheduled.length, 1);
    }

    #[test]
    fn test_process_task_to_run_without_a_task() {
        let class = empty_process_class("A");
        let mut process = OwnedProcess::new(Process::alloc(*class));

        assert!(process.task_to_run().is_none());
    }

    #[test]
    fn test_process_task_to_run_waiting_server() {
        let class = empty_process_class("A");
        let mut process = OwnedProcess::new(Process::alloc(*class));

        assert!(process.task_to_run().is_none());
        assert!(process.state().status.is_waiting_for_message());
        assert!(!process.state().status.is_waiting());
    }

    #[test]
    fn test_process_task_to_run_with_message() {
        let proc_class = OwnedClass::new(Class::process("A".to_string(), 0, 1));
        let method = empty_method();
        let future = FutureState::new();
        let index = MethodIndex::new(0);

        unsafe { proc_class.set_method(index, method) };

        let mut process = OwnedProcess::new(Process::alloc(*proc_class));

        process.send_async_message(index, future, vec![Pointer::int(42)]);

        let mut task = process.task_to_run().unwrap();

        assert!(process.task.is_some());
        assert!(task.stack.pop() == Some(Pointer::int(42)));
    }

    #[test]
    fn test_process_task_to_run_with_existing_task() {
        let proc_class = OwnedClass::new(Class::process("A".to_string(), 0, 1));
        let method = empty_method();
        let ctx = ExecutionContext::new(method);
        let task = Task::new(ctx, Write::Discard);
        let mut process = OwnedProcess::new(Process::alloc(*proc_class));

        process.task = Some(task);

        assert_eq!(
            process.task_to_run(),
            process.task.as_ref().map(TaskPointer::new)
        );

        Method::drop_and_deallocate(method);
    }

    #[test]
    fn test_finish_task_with_existing_task() {
        let proc_class = OwnedClass::new(Class::process("A".to_string(), 0, 1));
        let method = empty_method();
        let ctx = ExecutionContext::new(method);
        let task = Task::new(ctx, Write::Discard);
        let mut process = OwnedProcess::new(Process::alloc(*proc_class));

        process.task = Some(task);
        process.finish_task();

        assert!(process.task.is_none());

        Method::drop_and_deallocate(method);
    }

    #[test]
    fn test_process_finish_task_without_pending_work() {
        let class = empty_process_class("A");
        let mut process = OwnedProcess::new(Process::alloc(*class));

        process.set_main();

        assert!(!process.finish_task());
    }

    #[test]
    fn test_process_finish_task_with_clients() {
        let class = empty_process_class("A");
        let mut process = OwnedProcess::new(Process::alloc(*class));

        assert!(!process.finish_task());
        assert!(process.state().status.is_waiting_for_message());
    }

    #[test]
    fn test_process_finish_task_with_messages() {
        let proc_class = OwnedClass::new(Class::process("A".to_string(), 0, 1));
        let method = empty_method();
        let future = FutureState::new();
        let index = MethodIndex::new(0);

        unsafe { proc_class.set_method(index, method) };

        let mut process = OwnedProcess::new(Process::alloc(*proc_class));

        process.send_async_message(index, future, Vec::new());

        assert!(process.finish_task());
    }

    #[test]
    fn test_process_get_set_field() {
        let class = OwnedClass::new(Class::process("A".to_string(), 1, 0));
        let mut process = OwnedProcess::new(Process::alloc(*class));
        let idx = FieldIndex::new(0);

        unsafe {
            process.set_field(idx, Pointer::int(4));

            assert!(process.get_field(idx) == Pointer::int(4));
        }
    }

    #[test]
    fn test_process_timeout_expired() {
        let class = empty_process_class("A");
        let mut process = OwnedProcess::new(Process::alloc(*class));
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        assert!(!process.timeout_expired());

        process.suspend(timeout);

        assert!(!process.timeout_expired());
        assert!(!process.state().status.timeout_expired());
    }

    #[test]
    fn test_process_pointer_identifier() {
        let ptr = unsafe { ProcessPointer::new(0x4 as *mut _) };

        assert_eq!(ptr.identifier(), 0x4);
    }

    #[test]
    fn test_process_pointer_as_pointer() {
        let ptr = unsafe { ProcessPointer::new(0x4 as *mut _) };

        assert_eq!(ptr.as_pointer(), Pointer::new(0x4 as *mut _));
    }

    #[test]
    fn test_future_new() {
        let fut_class = empty_class("Future");
        let state = FutureState::new();
        let future = Future::alloc(*fut_class, state);

        unsafe {
            assert_eq!(
                future.get::<Header>().class.as_pointer(),
                fut_class.as_pointer()
            );
        }

        unsafe {
            Future::drop(future);
            future.free();
        }
    }

    #[test]
    fn test_future_write_without_consumer() {
        let state = FutureState::new();
        let result = state.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::Continue);
        assert!(!state.lock().unwrap().thrown);
    }

    #[test]
    fn test_future_write_thrown() {
        let state = FutureState::new();
        let result = state.write(Pointer::int(42), true);

        assert_eq!(result, WriteResult::Continue);
        assert!(state.lock().unwrap().thrown);
    }

    #[test]
    fn test_future_write_disconnected() {
        let state = FutureState::new();

        state.lock().unwrap().disconnected = true;

        let result = state.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::Discard);
    }

    #[test]
    fn test_future_write_with_consumer() {
        let proc_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(*proc_class));
        let state = FutureState::new();

        state.lock().unwrap().consumer = Some(*process);

        let result = state.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::Continue);
    }

    #[test]
    fn test_future_write_with_waiting_consumer() {
        let proc_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(*proc_class));
        let state = FutureState::new();

        process.state().waiting_for_future(None);
        state.lock().unwrap().consumer = Some(*process);

        let result = state.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::Reschedule(*process));
    }

    #[test]
    fn test_future_write_with_waiting_consumer_with_timeout() {
        let proc_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(*proc_class));
        let state = FutureState::new();
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        process.state().waiting_for_future(Some(timeout));
        state.lock().unwrap().consumer = Some(*process);

        let result = state.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::RescheduleWithTimeout(*process));
        assert!(!process.state().status.is_waiting_for_future());
        assert!(process.state().timeout.is_none());
    }

    #[test]
    fn test_future_get_without_result() {
        let proc_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(*proc_class));
        let state = FutureState::new();

        assert_eq!(state.get(*process, None), FutureResult::None);
        assert_eq!(state.lock().unwrap().consumer, Some(*process));
        assert!(process.state().status.is_waiting_for_future());
    }

    #[test]
    fn test_future_get_without_result_with_timeout() {
        let proc_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(*proc_class));
        let state = FutureState::new();
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        assert_eq!(state.get(*process, Some(timeout)), FutureResult::None);
        assert_eq!(state.lock().unwrap().consumer, Some(*process));
        assert!(process.state().status.is_waiting_for_future());
        assert!(process.state().timeout.is_some());
    }

    #[test]
    fn test_future_get_with_result() {
        let proc_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(*proc_class));
        let state = FutureState::new();
        let value = Pointer::int(42);

        process.state().waiting_for_future(None);
        state.lock().unwrap().result = value;

        assert_eq!(state.get(*process, None), FutureResult::Returned(value));
        assert!(state.lock().unwrap().consumer.is_none());
        assert!(!process.state().status.is_waiting_for_future());
    }

    #[test]
    fn test_future_get_with_thrown_result() {
        let proc_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(*proc_class));
        let state = FutureState::new();
        let value = Pointer::int(42);

        process.state().waiting_for_future(None);
        state.lock().unwrap().result = value;
        state.lock().unwrap().thrown = true;

        assert_eq!(state.get(*process, None), FutureResult::Thrown(value));
        assert!(state.lock().unwrap().consumer.is_none());
        assert!(!process.state().status.is_waiting_for_future());
    }

    #[test]
    fn test_future_disconnect() {
        let fut_class = empty_class("Future");
        let state = FutureState::new();
        let fut = Future::alloc(*fut_class, state.clone());
        let result = unsafe { fut.get::<Future>() }.disconnect();

        assert!(state.lock().unwrap().disconnected);
        assert!(result == Pointer::undefined_singleton());

        unsafe {
            Future::drop(fut);
            fut.free();
        }
    }

    #[test]
    fn test_process_stacktrace() {
        let proc_class = empty_process_class("B");
        let mut proc = new_process(*proc_class);
        let method1 = empty_method();
        let method2 = empty_method();

        unsafe {
            let m1 = method1.as_pointer().get_mut::<Method>();
            let m2 = method2.as_pointer().get_mut::<Method>();

            m1.locations.add_entry(0, 4, Pointer::int(2), Pointer::int(1));
            m2.locations.add_entry(0, 12, Pointer::int(4), Pointer::int(3));
        }

        let ctx1 = ExecutionContext::new(method1);
        let ctx2 = ExecutionContext::new(method2);
        let mut task = Task::new(ctx1, Write::Discard);

        task.push_context(ctx2);

        proc.task = Some(task);

        let trace = proc.stacktrace();

        assert_eq!(trace.len(), 2);
        assert_eq!(
            trace.get(0),
            Some(&Location {
                name: Pointer::int(1),
                file: Pointer::int(2),
                line: Pointer::int(4)
            })
        );
        assert_eq!(
            trace.get(1),
            Some(&Location {
                name: Pointer::int(3),
                file: Pointer::int(4),
                line: Pointer::int(12)
            })
        );

        Method::drop_and_deallocate(method1);
        Method::drop_and_deallocate(method2);
    }
}
