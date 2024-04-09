use crate::arc_without_weak::ArcWithoutWeak;
use crate::mem::{allocate, header_of, ClassPointer, Header};
use crate::scheduler::process::Thread;
use crate::scheduler::timeouts::Timeout;
use crate::stack::Stack;
use crate::state::State;
use backtrace;
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::mem::{align_of, forget, size_of, ManuallyDrop};
use std::ops::Drop;
use std::ops::{Deref, DerefMut};
use std::ptr::{drop_in_place, null_mut, write, NonNull};
use std::slice;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard};

const INKO_SYMBOL_IDENTIFIER: &str = "_IM_";

/// The type signature for Inko's async methods defined in the native code.
///
/// Native async methods only take a single argument: a `context::Context` that
/// contains all the necessary data. This makes it easier to pass multiple
/// values back to the native function without having to change the assembly
/// code used for context switching.
///
/// The argument this function takes is a Context. We use an opague pointer here
/// because a Context contains a State, which isn't FFI safe. This however is
/// fine as the State type is exposed as an opague pointer and its fields are
/// never read directly from native code.
///
/// While we can disable the lint on a per-function basis, this would require
/// annotating _a lot_ of functions, so instead we use an opague pointer here.
pub(crate) type NativeAsyncMethod = unsafe extern "system" fn(*mut u8);

/// A single stack frame in a process' call stack.
#[repr(C)]
pub struct StackFrame {
    pub name: String,
    pub path: String,
    pub line: i64,
}

/// A message sent between two processes.
#[repr(C)]
pub struct Message {
    /// The native function to run.
    pub method: NativeAsyncMethod,

    /// The number of arguments of this message.
    pub length: u8,

    /// The arguments of the message.
    pub arguments: [*mut u8; 0],
}

impl Message {
    pub(crate) fn alloc(method: NativeAsyncMethod, length: u8) -> OwnedMessage {
        unsafe {
            let layout = Self::layout(length);
            let raw_ptr = alloc(layout) as *mut Self;

            if raw_ptr.is_null() {
                handle_alloc_error(layout);
            }

            let msg = &mut *raw_ptr;

            init!(msg.method => method);
            init!(msg.length => length);

            OwnedMessage(NonNull::new_unchecked(raw_ptr))
        }
    }

    unsafe fn layout(length: u8) -> Layout {
        let size = size_of::<Self>() + (length as usize * size_of::<*mut u8>());

        // Messages are sent often, so we don't want the overhead of size and
        // alignment checks.
        Layout::from_size_align_unchecked(size, align_of::<Self>())
    }
}

#[repr(C)]
pub(crate) struct OwnedMessage(NonNull<Message>);

impl OwnedMessage {
    pub(crate) unsafe fn from_raw(message: *mut Message) -> OwnedMessage {
        OwnedMessage(NonNull::new_unchecked(message))
    }

    pub(crate) fn into_raw(mut self) -> *mut Message {
        let ptr = unsafe { self.0.as_mut() };

        forget(self);
        ptr
    }
}

impl Deref for OwnedMessage {
    type Target = Message;

    fn deref(&self) -> &Message {
        unsafe { self.0.as_ref() }
    }
}

impl DerefMut for OwnedMessage {
    fn deref_mut(&mut self) -> &mut Message {
        unsafe { self.0.as_mut() }
    }
}

impl Drop for OwnedMessage {
    fn drop(&mut self) {
        unsafe {
            let layout = Message::layout(self.0.as_ref().length);

            drop_in_place(self.0.as_ptr());
            dealloc(self.0.as_ptr() as *mut u8, layout);
        }
    }
}

/// A collection of messages to be processed by a process.
struct Mailbox {
    messages: VecDeque<OwnedMessage>,
}

impl Mailbox {
    fn new() -> Self {
        Mailbox { messages: VecDeque::new() }
    }

    fn send(&mut self, message: OwnedMessage) {
        self.messages.push_back(message);
    }

    fn receive(&mut self) -> Option<OwnedMessage> {
        self.messages.pop_front()
    }
}

pub(crate) enum Task {
    Resume,
    Start(NativeAsyncMethod, Vec<*mut u8>),
    Wait,
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

    /// The process is waiting for a channel.
    const WAITING_FOR_CHANNEL: u8 = 0b00_0100;

    /// The process is waiting for an IO operation to complete.
    const WAITING_FOR_IO: u8 = 0b00_1000;

    /// The process is simply sleeping for a certain amount of time.
    const SLEEPING: u8 = 0b01_0000;

    /// The process was rescheduled after a timeout expired.
    const TIMEOUT_EXPIRED: u8 = 0b10_0000;

    /// The process is running a message.
    const RUNNING: u8 = 0b100_0000;

    /// The process is waiting for something, or suspended for a period of time.
    const WAITING: u8 =
        Self::WAITING_FOR_CHANNEL | Self::SLEEPING | Self::WAITING_FOR_IO;

    pub(crate) fn new() -> Self {
        Self { bits: Self::NORMAL }
    }

    fn set_main(&mut self) {
        self.update_bits(Self::MAIN, true);
    }

    fn is_running(&self) -> bool {
        self.bit_is_set(Self::RUNNING)
    }

    fn set_running(&mut self, enable: bool) {
        self.update_bits(Self::RUNNING, enable);
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

    fn set_waiting_for_channel(&mut self, enable: bool) {
        self.update_bits(Self::WAITING_FOR_CHANNEL, enable);
    }

    fn set_waiting_for_io(&mut self, enable: bool) {
        self.update_bits(Self::WAITING_FOR_IO, enable);
    }

    fn is_waiting_for_io(&self) -> bool {
        self.bit_is_set(Self::WAITING_FOR_IO)
    }

    fn is_waiting_for_channel(&self) -> bool {
        self.bit_is_set(Self::WAITING_FOR_CHANNEL)
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

    pub(crate) fn suspend(&mut self, timeout: ArcWithoutWeak<Timeout>) {
        self.timeout = Some(timeout);
        self.status.set_sleeping(true);
    }

    pub(crate) fn try_reschedule_after_timeout(&mut self) -> RescheduleRights {
        if !self.status.is_waiting() {
            return RescheduleRights::Failed;
        }

        if self.status.is_waiting_for_channel()
            || self.status.is_waiting_for_io()
        {
            // We may be suspended for some time without actually waiting for
            // anything, in that case we don't want to update the process
            // status.
            self.status.set_timeout_expired(true);
        }

        self.status.no_longer_waiting();

        if self.timeout.take().is_some() {
            RescheduleRights::AcquiredWithTimeout
        } else {
            RescheduleRights::Acquired
        }
    }

    pub(crate) fn waiting_for_channel(
        &mut self,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) {
        self.timeout = timeout;

        self.status.set_waiting_for_channel(true);
    }

    pub(crate) fn waiting_for_io(
        &mut self,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) {
        self.timeout = timeout;

        self.status.set_waiting_for_io(true);
    }

    fn try_reschedule_for_message(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_message() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_message(false);
        RescheduleRights::Acquired
    }

    fn try_reschedule_for_channel(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_channel() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_channel(false);

        if self.timeout.take().is_some() {
            RescheduleRights::AcquiredWithTimeout
        } else {
            RescheduleRights::Acquired
        }
    }

    pub(crate) fn try_reschedule_for_io(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_io() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_io(false);

        if self.timeout.take().is_some() {
            RescheduleRights::AcquiredWithTimeout
        } else {
            RescheduleRights::Acquired
        }
    }
}

/// Data about a process stored in its stack.
///
/// This data is stored in the stack such that the generated code can easily
/// retrieve it, without needing thread-locals, globals, etc.
#[repr(C)]
pub struct StackData {
    /// A pointer back to the process that owns this data.
    pub process: ProcessPointer,

    /// A pointer to the thread that is running this process.
    ///
    /// The value of this field is only relevant/correct when the process is in
    /// fact running.
    pub thread: *mut Thread,

    /// The scheduler epoch at which this process started running.
    pub started_at: u32,
}

/// A lightweight process.
#[repr(C)]
pub struct Process {
    pub header: Header,

    /// A lock acquired when running a process.
    ///
    /// Processes may perform operations that result in the process being
    /// rescheduled by another thread. An example is when process A sends a
    /// message to process B and wants to wait for it, but B reschedules A
    /// before A finishes wrapping up and suspends itself.
    ///
    /// The run lock is meant to prevent the process from running more than
    /// once, and makes implementing various aspects (e.g. sending messages)
    /// easier and safe.
    ///
    /// This lock _is not_ used to access the shared state of a process.
    ///
    /// This lock is separate from the other inner fields so that native code
    /// can mutate these while the lock is held, without having to explicitly
    /// acquire the lock every time.
    ///
    /// This value is wrapped in an UnsafeCell so we can borrow it without
    /// running into borrowing conflicts with methods or other fields. An
    /// alternative would be to move the process-local portion into a separate
    /// type and define the necessary methods on that type, instead of defining
    /// them on `Process` directly. We actually used such an approach in the
    /// past, but found it to be rather clunky to work with.
    pub(crate) run_lock: UnsafeCell<Mutex<()>>,

    /// The current stack pointer.
    ///
    /// When this pointer is set to NULL it means the process no longer has an
    /// associated stack.
    ///
    /// When this process is suspended, this pointer is the current stack
    /// pointer of this process. When the process is running it instead is the
    /// stack pointer of the thread that switched to running the process.
    ///
    /// The stack pointer is reset every time a new message is picked up.
    pub(crate) stack_pointer: *mut u8,

    /// The stack memory of this process.
    ///
    /// This value may be absent, in which case `stack_pointer` is set to NULL.
    /// We take this approach in order to keep processes as small as possible,
    /// and to remove the need for unwrapping an `Option` every time we know for
    /// certain a stack is present.
    pub(crate) stack: ManuallyDrop<Stack>,

    /// The shared state of the process.
    ///
    /// Multiple processes/threads may try to access this state, such as when
    /// they are sending a message to this process. Access to this data doesn't
    /// one obtains the run lock first.
    state: Mutex<ProcessState>,

    /// The fields of this process.
    ///
    /// The length of this flexible array is derived from the number of fields
    /// defined in this process' class.
    pub fields: [*mut u8; 0],
}

impl Process {
    pub(crate) fn drop_and_deallocate(ptr: ProcessPointer) {
        unsafe {
            let raw = ptr.as_ptr();
            let layout = header_of(raw).class.instance_layout();

            drop_in_place(raw);
            dealloc(raw as *mut u8, layout);
        }
    }

    pub(crate) fn alloc(class: ClassPointer, stack: Stack) -> ProcessPointer {
        let ptr = allocate(unsafe { class.instance_layout() }) as *mut Self;
        let obj = unsafe { &mut *ptr };
        let mut state = ProcessState::new();

        // Processes start without any messages, so we must ensure their status
        // is set accordingly.
        state.status.set_waiting_for_message(true);

        unsafe {
            write(
                stack.private_data_pointer() as *mut StackData,
                StackData {
                    process: ProcessPointer::new(ptr),
                    started_at: 0,
                    thread: null_mut(),
                },
            );
        }

        obj.header.init_atomic(class);
        init!(obj.run_lock => UnsafeCell::new(Mutex::new(())));
        init!(obj.stack_pointer => stack.stack_pointer());
        init!(obj.stack => ManuallyDrop::new(stack));
        init!(obj.state => Mutex::new(state));

        unsafe { ProcessPointer::new(ptr) }
    }

    /// Returns a new Process acting as the main process.
    ///
    /// This process always runs on the main thread.
    pub(crate) fn main(
        class: ClassPointer,
        method: NativeAsyncMethod,
        stack: Stack,
    ) -> ProcessPointer {
        let mut process = Self::alloc(class, stack);
        let message = Message::alloc(method, 0);

        process.set_main();
        process.send_message(message);
        process
    }

    pub(crate) fn set_main(&mut self) {
        self.state.lock().unwrap().status.set_main();
    }

    pub(crate) fn is_main(&self) -> bool {
        self.state.lock().unwrap().status.is_main()
    }

    /// Sends a synchronous message to this process.
    pub(crate) fn send_message(
        &mut self,
        message: OwnedMessage,
    ) -> RescheduleRights {
        let mut state = self.state.lock().unwrap();

        state.mailbox.send(message);
        state.try_reschedule_for_message()
    }

    pub(crate) fn next_task(&mut self) -> Task {
        let mut state = self.state.lock().unwrap();

        if state.status.is_running() {
            return Task::Resume;
        }

        let message = {
            if let Some(message) = state.mailbox.receive() {
                message
            } else {
                state.status.set_waiting_for_message(true);
                return Task::Wait;
            }
        };

        let func = message.method;
        let len = message.length as usize;
        let args = unsafe {
            slice::from_raw_parts(message.arguments.as_ptr(), len).to_vec()
        };

        self.stack_pointer = self.stack.stack_pointer();
        state.status.set_running(true);
        Task::Start(func, args)
    }

    pub(crate) fn take_stack(&mut self) -> Option<Stack> {
        if self.stack_pointer.is_null() {
            None
        } else {
            self.stack_pointer = null_mut();

            Some(unsafe { ManuallyDrop::take(&mut self.stack) })
        }
    }

    /// Finishes the exection of a message, and decides what to do next with
    /// this process.
    ///
    /// If the return value is `true`, the process should be rescheduled.
    pub(crate) fn finish_message(&mut self) -> bool {
        let mut state = self.state.lock().unwrap();

        // We must clear this status so we pick up the next message when
        // rescheduling the process at some point in the future.
        state.status.set_running(false);

        if state.mailbox.messages.is_empty() {
            state.status.set_waiting_for_message(true);
            false
        } else {
            true
        }
    }

    pub(crate) fn clear_timeout(&self) {
        self.state.lock().unwrap().status.set_timeout_expired(false);
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

    /// Acquires the run lock of this process.
    ///
    /// We use an explicit lifetime here so the mutex guard's lifetime isn't
    /// bound to `self`, allowing us to borrow it while also borrowing other
    /// parts of a process.
    pub(crate) fn acquire_run_lock<'a>(&self) -> MutexGuard<'a, ()> {
        // Safety: the lock itself is always present, we just use UnsafeCell so
        // we can borrow _just_ the lock while still being able to borrow the
        // rest of the process through methods.
        unsafe { (*self.run_lock.get()).lock().unwrap() }
    }

    /// Returns a mutable reference to the thread that's running this process.
    ///
    /// This method is unsafe as it assumes a thread is set, and the pointer
    /// points to valid data.
    ///
    /// The lifetime of the returned reference isn't bound to `self` as doing so
    /// prevents various patterns we depend on (e.g.
    /// `process.thread().schedule(process)`). In addition, the reference itself
    /// remains valid even when moving the process around, as a thread always
    /// outlives a process.
    pub(crate) unsafe fn thread<'a>(&mut self) -> &'a mut Thread {
        &mut *self.stack_data().thread
    }

    pub(crate) fn stacktrace(&self) -> Vec<StackFrame> {
        let mut frames = Vec::new();

        // We don't use backtrace::trace() so we can avoid the frames introduced
        // by calling this function (and any functions it may call).
        let trace = backtrace::Backtrace::new();

        for frame in trace.frames() {
            backtrace::resolve(frame.ip(), |symbol| {
                let name = if let Some(sym_name) = symbol.name() {
                    // We only want to include frames for Inko source code, not
                    // any additional frames introduced by the runtime library
                    // and its dependencies.
                    let base = if let Some(name) = sym_name
                        .as_str()
                        .unwrap_or("")
                        .strip_prefix(INKO_SYMBOL_IDENTIFIER)
                    {
                        name
                    } else {
                        return;
                    };

                    // Methods include the shape identifiers to prevent name
                    // conflicts. We get rid of these to ensure the stacktraces
                    // are easier to understand.
                    if let Some(idx) = base.find('#') {
                        base[0..idx].to_string()
                    } else {
                        base.to_string()
                    }
                } else {
                    String::new()
                };

                let path = symbol
                    .filename()
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default();

                let line = symbol.lineno().unwrap_or(0) as i64;

                frames.push(StackFrame { name, path, line });
            });
        }

        frames.reverse();
        frames
    }

    pub(crate) fn resume(&mut self, state: &State, thread: &mut Thread) {
        let data = self.stack_data();

        data.started_at = state.scheduler_epoch.load(Ordering::Relaxed);
        data.thread = thread as *mut _;
    }

    /// Returns a mutable reference to the process state stored on its stack.
    ///
    /// This method is safe because `self.stack` always points to the stack
    /// page, regardless of whether or not the process is running. We also
    /// ensure the data is set in `Process::alloc`.
    pub(crate) fn stack_data(&mut self) -> &mut StackData {
        unsafe { &mut *(self.stack.private_data_pointer() as *mut StackData) }
    }
}

/// A pointer to a process.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ProcessPointer(NonNull<Process>);

unsafe impl Sync for ProcessPointer {}
unsafe impl Send for ProcessPointer {}

impl ProcessPointer {
    pub(crate) unsafe fn new(pointer: *mut Process) -> Self {
        Self(NonNull::new_unchecked(pointer))
    }

    pub(crate) fn as_ptr(self) -> *mut Process {
        self.0.as_ptr()
    }

    pub(crate) fn identifier(self) -> usize {
        self.as_ptr() as usize
    }

    pub(crate) fn blocking<R>(mut self, function: impl FnOnce() -> R) -> R {
        // Safety: threads are stored in processes before running them.
        unsafe { self.thread().blocking(self, function) }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if !self.stack_pointer.is_null() {
            unsafe {
                ManuallyDrop::drop(&mut self.stack);
            }
        }
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

#[derive(Eq, PartialEq, Debug)]
pub(crate) enum SendResult {
    Sent,
    Full,
    Reschedule(ProcessPointer),
    RescheduleWithTimeout(ProcessPointer),
}

#[derive(Eq, PartialEq, Debug)]
pub(crate) enum ReceiveResult {
    None,
    Some(*mut u8),
    Reschedule(*mut u8, ProcessPointer),
}

/// The internal (synchronised) state of a channel.
pub(crate) struct ChannelState {
    /// The index into the ring buffer to use for sending a new value.
    send_index: usize,

    /// The index into the ring buffer to use for receiving a value.
    receive_index: usize,

    /// The fixed-size ring buffer of messages.
    ///
    /// NULL is a valid value, as Inko uses 0x0/0 for `nil` and `false`.
    messages: Box<[*mut u8]>,

    /// The number of messages in the channel.
    len: usize,

    /// Processes waiting for a message to be sent to this channel.
    waiting_for_message: Vec<ProcessPointer>,

    /// Processes that tried to send a message when the channel was full.
    waiting_for_space: Vec<ProcessPointer>,
}

impl ChannelState {
    fn new(capacity: usize) -> ChannelState {
        ChannelState {
            messages: (0..capacity).map(|_| null_mut()).collect(),
            len: 0,
            send_index: 0,
            receive_index: 0,
            waiting_for_message: Vec::new(),
            waiting_for_space: Vec::new(),
        }
    }

    fn capacity(&self) -> usize {
        self.messages.len()
    }

    fn is_full(&self) -> bool {
        self.capacity() == self.len
    }

    fn send(&mut self, value: *mut u8) -> bool {
        if self.is_full() {
            return false;
        }

        let index = self.send_index;

        self.messages[index] = value;
        self.send_index = self.next_index(index);
        self.len += 1;
        true
    }

    fn receive(&mut self) -> Option<*mut u8> {
        if self.len == 0 {
            return None;
        }

        let index = self.receive_index;
        let value = self.messages[index];

        self.receive_index = self.next_index(index);
        self.len -= 1;
        Some(value)
    }

    fn next_index(&self, index: usize) -> usize {
        // The & operator can't be used as we don't guarantee/require message
        // sizes to be a power of two. The % operator is quite expensive to use:
        // a simple micro benchmark at the time of writing suggested that the %
        // operator is about three times slower compared to a branch like the
        // one here.
        if index == self.capacity() - 1 {
            0
        } else {
            index + 1
        }
    }
}

/// A multiple publisher, multiple consumer first-in-first-out channel.
///
/// Messages are sent and received in FIFO order. However, processes waiting for
/// messages or for space to be available (in case the channel is full) aren't
/// woken up in FIFO order. Currently this uses a LIFO order, but this isn't
/// guaranteed nor should this be relied upon.
///
/// Channels are not lock-free, and as such may perform worse compared to
/// channels found in other languages (e.g. Rust or Go). This is because in its
/// current form we favour simplicity and correctness over performance. This may
/// be improved upon in the future.
///
/// Channels are always bounded and have a minimum capacity of 1, even if the
/// user-specified capacity is 0. When a channel is full, processes sending
/// messages are to be suspended and woken up again when space is available.
#[repr(C)]
pub struct Channel {
    pub(crate) state: Mutex<ChannelState>,
}

impl Channel {
    pub(crate) fn new(capacity: usize) -> Channel {
        Channel { state: Mutex::new(ChannelState::new(capacity)) }
    }

    pub(crate) unsafe fn drop(ptr: *mut Channel) {
        drop_in_place(ptr);
    }

    pub(crate) fn send(
        &self,
        sender: ProcessPointer,
        message: *mut u8,
    ) -> SendResult {
        let mut state = self.state.lock().unwrap();

        if !state.send(message) {
            state.waiting_for_space.push(sender);
            return SendResult::Full;
        }

        if let Some(receiver) = state.waiting_for_message.pop() {
            // We don't need to keep the lock any longer than necessary.
            drop(state);

            // The process may be waiting for more than one channel to receive a
            // message. In this case it's possible that multiple different
            // processes try to reschedule the same waiting process, so we have
            // to acquire the rescheduling rights first.
            match receiver.state().try_reschedule_for_channel() {
                RescheduleRights::Failed => SendResult::Sent,
                RescheduleRights::Acquired => SendResult::Reschedule(receiver),
                RescheduleRights::AcquiredWithTimeout => {
                    SendResult::RescheduleWithTimeout(receiver)
                }
            }
        } else {
            SendResult::Sent
        }
    }

    pub(crate) fn receive(
        &self,
        receiver: ProcessPointer,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) -> ReceiveResult {
        let mut state = self.state.lock().unwrap();

        if let Some(msg) = state.receive() {
            if let Some(proc) = state.waiting_for_space.pop() {
                ReceiveResult::Reschedule(msg, proc)
            } else {
                ReceiveResult::Some(msg)
            }
        } else {
            receiver.state().waiting_for_channel(timeout);
            state.waiting_for_message.push(receiver);
            ReceiveResult::None
        }
    }

    pub(crate) fn try_receive(&self) -> ReceiveResult {
        let mut state = self.state.lock().unwrap();

        if let Some(msg) = state.receive() {
            if let Some(proc) = state.waiting_for_space.pop() {
                ReceiveResult::Reschedule(msg, proc)
            } else {
                ReceiveResult::Some(msg)
            }
        } else {
            ReceiveResult::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{empty_process_class, setup, OwnedProcess};
    use rustix::param::page_size;
    use std::time::Duration;

    macro_rules! offset_of {
        ($value: expr, $field: ident) => {{
            (std::ptr::addr_of!($value.$field) as usize)
                .saturating_sub($value.0.as_ptr() as usize)
        }};
    }

    unsafe extern "system" fn method(_ctx: *mut u8) {
        // This function is used for testing the sending/receiving of messages.
    }

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<Message>(), 16);
        assert_eq!(size_of::<ManuallyDrop<Stack>>(), 16);
        assert!(size_of::<StackData>() <= page_size());

        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            assert_eq!(size_of::<UnsafeCell<Mutex<()>>>(), 8);
            assert_eq!(size_of::<Process>(), 104);
            assert_eq!(size_of::<Channel>(), 96);
        } else {
            assert_eq!(size_of::<UnsafeCell<Mutex<()>>>(), 16);
            assert_eq!(size_of::<Process>(), 120);
            assert_eq!(size_of::<Channel>(), 104);
        }

        assert_eq!(size_of::<ProcessState>(), 48);
        assert_eq!(size_of::<Option<NonNull<Thread>>>(), 8);
        assert_eq!(size_of::<ChannelState>(), 88);
    }

    #[test]
    fn test_field_offsets() {
        let proc_class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let proc = OwnedProcess::new(Process::alloc(*proc_class, stack));

        assert_eq!(offset_of!(proc, header), 0);
        assert_eq!(
            offset_of!(proc, fields),
            if cfg!(any(target_os = "linux", target_os = "freebsd")) {
                104
            } else {
                120
            }
        );
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
    fn test_process_status_set_waiting_for_channel() {
        let mut status = ProcessStatus::new();

        status.set_waiting_for_channel(true);
        assert!(status.is_waiting_for_channel());

        status.set_waiting_for_channel(false);
        assert!(!status.is_waiting_for_channel());
    }

    #[test]
    fn test_process_status_no_longer_waiting() {
        let mut status = ProcessStatus::new();

        status.set_running(true);
        status.set_waiting_for_channel(true);
        status.set_waiting_for_io(true);
        status.set_sleeping(true);
        status.no_longer_waiting();

        assert!(status.is_running());
        assert!(!status.is_waiting_for_channel());
        assert!(!status.is_waiting_for_io());
        assert!(!status.bit_is_set(ProcessStatus::SLEEPING));
        assert!(!status.is_waiting());
    }

    #[test]
    fn test_process_status_is_waiting() {
        let mut status = ProcessStatus::new();

        status.set_sleeping(true);
        assert!(status.is_waiting());

        status.set_sleeping(false);
        status.set_waiting_for_channel(true);
        assert!(status.is_waiting());

        status.no_longer_waiting();

        assert!(!status.is_waiting_for_channel());
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
        let state = setup();
        let mut proc_state = ProcessState::new();
        let timeout = Timeout::duration(&state, Duration::from_secs(0));

        assert!(!proc_state.has_same_timeout(&timeout));

        proc_state.timeout = Some(timeout.clone());

        assert!(proc_state.has_same_timeout(&timeout));
    }

    #[test]
    fn test_process_state_try_reschedule_after_timeout() {
        let state = setup();
        let mut proc_state = ProcessState::new();

        assert_eq!(
            proc_state.try_reschedule_after_timeout(),
            RescheduleRights::Failed
        );

        proc_state.waiting_for_channel(None);

        assert_eq!(
            proc_state.try_reschedule_after_timeout(),
            RescheduleRights::Acquired
        );

        assert!(!proc_state.status.is_waiting_for_channel());
        assert!(!proc_state.status.is_waiting());

        let timeout = Timeout::duration(&state, Duration::from_secs(0));

        proc_state.waiting_for_channel(Some(timeout));

        assert_eq!(
            proc_state.try_reschedule_after_timeout(),
            RescheduleRights::AcquiredWithTimeout
        );

        assert!(!proc_state.status.is_waiting_for_channel());
        assert!(!proc_state.status.is_waiting());
    }

    #[test]
    fn test_process_state_waiting_for_channel() {
        let state = setup();
        let mut proc_state = ProcessState::new();
        let timeout = Timeout::duration(&state, Duration::from_secs(0));

        proc_state.waiting_for_channel(None);

        assert!(proc_state.status.is_waiting_for_channel());
        assert!(proc_state.timeout.is_none());

        proc_state.waiting_for_channel(Some(timeout));

        assert!(proc_state.status.is_waiting_for_channel());
        assert!(proc_state.timeout.is_some());
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
    fn test_process_state_try_reschedule_for_channel() {
        let state = setup();
        let mut proc_state = ProcessState::new();

        assert_eq!(
            proc_state.try_reschedule_for_channel(),
            RescheduleRights::Failed
        );

        proc_state.status.set_waiting_for_channel(true);
        assert_eq!(
            proc_state.try_reschedule_for_channel(),
            RescheduleRights::Acquired
        );
        assert!(!proc_state.status.is_waiting_for_channel());

        proc_state.status.set_waiting_for_channel(true);
        proc_state.timeout =
            Some(Timeout::duration(&state, Duration::from_secs(0)));

        assert_eq!(
            proc_state.try_reschedule_for_channel(),
            RescheduleRights::AcquiredWithTimeout
        );
        assert!(!proc_state.status.is_waiting_for_channel());
    }

    #[test]
    fn test_process_new() {
        let class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(
            *class,
            Stack::new(32, page_size()),
        ));

        assert_eq!(process.header.class, class.0);
    }

    #[test]
    fn test_process_main() {
        let proc_class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let process =
            OwnedProcess::new(Process::main(*proc_class, method, stack));

        assert!(process.is_main());
    }

    #[test]
    fn test_process_set_main() {
        let class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let mut process = OwnedProcess::new(Process::alloc(*class, stack));

        assert!(!process.is_main());

        process.set_main();
        assert!(process.is_main());
    }

    #[test]
    fn test_process_state_suspend() {
        let state = setup();
        let class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let process = OwnedProcess::new(Process::alloc(*class, stack));
        let timeout = Timeout::duration(&state, Duration::from_secs(0));

        process.state().suspend(timeout);

        assert!(process.state().timeout.is_some());
        assert!(process.state().status.is_waiting());
    }

    #[test]
    fn test_process_timeout_expired() {
        let state = setup();
        let class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let process = OwnedProcess::new(Process::alloc(*class, stack));
        let timeout = Timeout::duration(&state, Duration::from_secs(0));

        assert!(!process.timeout_expired());

        process.state().suspend(timeout);

        assert!(!process.timeout_expired());
        assert!(!process.state().status.timeout_expired());
    }

    #[test]
    fn test_process_pointer_identifier() {
        let ptr = unsafe { ProcessPointer::new(0x4 as *mut _) };

        assert_eq!(ptr.identifier(), 0x4);
    }

    #[test]
    fn test_channel_alloc() {
        let chan = Channel::new(4);
        let state = chan.state.lock().unwrap();

        assert_eq!(state.messages.len(), 4);
    }

    #[test]
    fn test_channel_send_empty() {
        let process_class = empty_process_class("A");
        let sender = OwnedProcess::new(Process::alloc(
            *process_class,
            Stack::new(32, page_size()),
        ));
        let chan = Channel::new(4);
        let msg = 42;

        assert_eq!(chan.send(*sender, msg as _), SendResult::Sent);
    }

    #[test]
    fn test_channel_send_full() {
        let process_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(
            *process_class,
            Stack::new(32, page_size()),
        ));
        let chan = Channel::new(1);
        let msg = 42;

        assert_eq!(chan.send(*process, msg as _), SendResult::Sent);
        assert_eq!(chan.send(*process, msg as _), SendResult::Full);
    }

    #[test]
    fn test_channel_send_with_waiting() {
        let process_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(
            *process_class,
            Stack::new(32, page_size()),
        ));
        let chan = Channel::new(1);
        let msg = 42;

        chan.receive(*process, None);

        assert_eq!(
            chan.send(*process, msg as _),
            SendResult::Reschedule(*process)
        );
    }

    #[test]
    fn test_channel_send_with_waiting_with_timeout() {
        let state = setup();
        let process_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(
            *process_class,
            Stack::new(32, page_size()),
        ));
        let chan = Channel::new(1);
        let msg = 42;

        chan.receive(
            *process,
            Some(Timeout::duration(&state, Duration::from_secs(0))),
        );

        assert_eq!(
            chan.send(*process, msg as _),
            SendResult::RescheduleWithTimeout(*process)
        );
    }

    #[test]
    fn test_channel_receive_empty() {
        let process_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(
            *process_class,
            Stack::new(32, page_size()),
        ));
        let chan = Channel::new(1);

        assert_eq!(chan.receive(*process, None), ReceiveResult::None);
        assert!(process.state().status.is_waiting_for_channel());
    }

    #[test]
    fn test_channel_try_receive_empty() {
        let process_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(
            *process_class,
            Stack::new(32, page_size()),
        ));
        let chan = Channel::new(1);

        assert_eq!(chan.try_receive(), ReceiveResult::None);
        assert!(!process.state().status.is_waiting_for_channel());
    }

    #[test]
    fn test_channel_receive_with_messages() {
        let process_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(
            *process_class,
            Stack::new(32, page_size()),
        ));
        let chan = Channel::new(1);
        let msg = 42;

        chan.send(*process, msg as _);

        assert_eq!(chan.receive(*process, None), ReceiveResult::Some(msg as _));
    }

    #[test]
    fn test_channel_receive_with_messages_with_blocked_sender() {
        let process_class = empty_process_class("A");
        let process = OwnedProcess::new(Process::alloc(
            *process_class,
            Stack::new(32, page_size()),
        ));
        let chan = Channel::new(1);
        let msg = 42;

        chan.send(*process, msg as _);
        chan.send(*process, msg as _);

        assert_eq!(
            chan.receive(*process, None),
            ReceiveResult::Reschedule(msg as _, *process)
        );
    }

    #[test]
    fn test_message_new() {
        let message = Message::alloc(method, 2);

        assert_eq!(message.length, 2);
    }

    #[test]
    fn test_mailbox_send() {
        let mut mail = Mailbox::new();
        let msg = Message::alloc(method, 0);

        mail.send(msg);
        assert!(mail.receive().is_some());
    }

    #[test]
    fn test_process_send_message() {
        let proc_class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let mut process = OwnedProcess::new(Process::alloc(*proc_class, stack));
        let msg = Message::alloc(method, 0);

        assert_eq!(process.send_message(msg), RescheduleRights::Acquired);
        assert_eq!(process.state().mailbox.messages.len(), 1);
    }

    #[test]
    fn test_process_next_task_without_messages() {
        let proc_class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let mut process = OwnedProcess::new(Process::alloc(*proc_class, stack));

        assert!(matches!(process.next_task(), Task::Wait));
    }

    #[test]
    fn test_process_next_task_with_new_message() {
        let proc_class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let mut process = OwnedProcess::new(Process::alloc(*proc_class, stack));
        let msg = Message::alloc(method, 0);

        process.send_message(msg);

        assert!(matches!(process.next_task(), Task::Start(_, _)));
    }

    #[test]
    fn test_process_next_task_with_existing_message() {
        let proc_class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let mut process = OwnedProcess::new(Process::alloc(*proc_class, stack));
        let msg1 = Message::alloc(method, 0);
        let msg2 = Message::alloc(method, 0);

        process.send_message(msg1);
        process.next_task();
        process.send_message(msg2);

        assert!(matches!(process.next_task(), Task::Resume));
    }

    #[test]
    fn test_process_take_stack() {
        let proc_class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let mut process = OwnedProcess::new(Process::alloc(*proc_class, stack));

        assert!(process.take_stack().is_some());
        assert!(process.stack_pointer.is_null());
    }

    #[test]
    fn test_process_finish_message() {
        let proc_class = empty_process_class("A");
        let stack = Stack::new(32, page_size());
        let mut process = OwnedProcess::new(Process::alloc(*proc_class, stack));

        assert!(!process.finish_message());
        assert!(process.state().status.is_waiting_for_message());
    }

    #[test]
    fn test_channel_state_send() {
        let mut state = ChannelState::new(2);

        assert!(!state.is_full());
        assert_eq!(state.capacity(), 2);

        assert!(state.send(0x1 as _));
        assert!(state.send(0x2 as _));
        assert!(!state.send(0x3 as _));
        assert!(!state.send(0x4 as _));

        assert_eq!(state.messages[0], 0x1 as _);
        assert_eq!(state.messages[1], 0x2 as _);
        assert!(state.is_full());
    }

    #[test]
    fn test_channel_state_receive() {
        let mut state = ChannelState::new(2);

        assert!(state.receive().is_none());

        state.send(0x1 as _);
        state.send(0x2 as _);

        assert!(state.is_full());
        assert_eq!(state.receive(), Some(0x1 as _));
        assert!(!state.is_full());

        assert_eq!(state.receive(), Some(0x2 as _));
        assert_eq!(state.receive(), None);
        assert!(!state.is_full());
    }
}
