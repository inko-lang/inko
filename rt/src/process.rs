use crate::config::Config;
use crate::mem::{allocate, header_of, Header, TypePointer};
use crate::scheduler::process::Thread;
use crate::scheduler::timeouts::Id as TimeoutId;
use crate::stack::Stack;
use crate::state::State;
use std::alloc::dealloc;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::ops::Drop;
use std::ops::{Deref, DerefMut};
use std::ptr::{drop_in_place, null_mut, write, NonNull};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard};

const METHOD_IDENTIFIER: &str = "_IM_";
const CLOSURE_IDENTIFIER: &str = "_IMC_";

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
pub(crate) struct Message {
    /// A pointer to the method to run.
    pub(crate) method: NativeAsyncMethod,

    /// A pointer to a structure containing the arguments of the message.
    ///
    /// This is managed by the standard library/generated code. If no arguments
    /// are passed, this field is set to NULL.
    pub(crate) data: *mut u8,
}

/// A collection of messages to be processed by a process.
struct Mailbox {
    messages: VecDeque<Message>,
}

impl Mailbox {
    fn new() -> Self {
        Mailbox { messages: VecDeque::with_capacity(4) }
    }

    fn send(&mut self, message: Message) {
        self.messages.push_back(message);
    }

    fn receive(&mut self) -> Option<Message> {
        self.messages.pop_front()
    }
}

pub(crate) enum Task {
    Resume,
    Start(Message),
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

    /// The process is waiting for a value to be sent over some data structure.
    const WAITING_FOR_VALUE: u8 = 0b00_0100;

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
        Self::WAITING_FOR_VALUE | Self::SLEEPING | Self::WAITING_FOR_IO;

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

    fn set_waiting_for_value(&mut self, enable: bool) {
        self.update_bits(Self::WAITING_FOR_VALUE, enable);
    }

    fn set_waiting_for_io(&mut self, enable: bool) {
        self.update_bits(Self::WAITING_FOR_IO, enable);
    }

    fn is_waiting_for_io(&self) -> bool {
        self.bit_is_set(Self::WAITING_FOR_IO)
    }

    fn is_waiting_for_value(&self) -> bool {
        self.bit_is_set(Self::WAITING_FOR_VALUE)
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
    AcquiredWithTimeout(TimeoutId),
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

    /// Additional bits that may be set by the network poller, the meaning of
    /// which depends on the context.
    poll_bits: u8,

    /// The ID of the timeout this process is suspended with, if any.
    ///
    /// If missing and the process is suspended, it means the process is
    /// suspended indefinitely.
    timeout: Option<TimeoutId>,
}

impl ProcessState {
    pub(crate) fn new() -> Self {
        Self {
            mailbox: Mailbox::new(),
            status: ProcessStatus::new(),
            poll_bits: 0,
            timeout: None,
        }
    }

    pub(crate) fn suspend(&mut self, timeout: TimeoutId) {
        self.timeout = Some(timeout);
        self.status.set_sleeping(true);
    }

    pub(crate) fn try_reschedule_after_timeout(&mut self) -> RescheduleRights {
        if !self.status.is_waiting() {
            return RescheduleRights::Failed;
        }

        if self.status.is_waiting_for_value() || self.status.is_waiting_for_io()
        {
            // We may be suspended for some time without actually waiting for
            // anything, in that case we don't want to update the process
            // status.
            self.status.set_timeout_expired(true);
        }

        self.status.no_longer_waiting();

        if let Some(id) = self.timeout.take() {
            RescheduleRights::AcquiredWithTimeout(id)
        } else {
            RescheduleRights::Acquired
        }
    }

    pub(crate) fn waiting_for_value(&mut self, timeout: Option<TimeoutId>) {
        self.timeout = timeout;
        self.status.set_waiting_for_value(true);
    }

    pub(crate) fn waiting_for_io(&mut self, timeout: Option<TimeoutId>) {
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

    pub(crate) fn try_reschedule_for_value(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_value() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_value(false);

        if let Some(id) = self.timeout.take() {
            RescheduleRights::AcquiredWithTimeout(id)
        } else {
            RescheduleRights::Acquired
        }
    }

    pub(crate) fn try_reschedule_for_io(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_io() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_io(false);

        if let Some(id) = self.timeout.take() {
            RescheduleRights::AcquiredWithTimeout(id)
        } else {
            RescheduleRights::Acquired
        }
    }

    pub(crate) fn set_poll_bit(&mut self, bit: u8) {
        self.poll_bits |= bit;
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
    /// When a process is suspended, this will be a pointer to the thread that
    /// last ran the process.
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
    /// defined in this process' type.
    pub fields: [*mut u8; 0],
}

impl Process {
    pub(crate) fn drop_and_deallocate(ptr: ProcessPointer) {
        unsafe {
            let raw = ptr.as_ptr();
            let layout = header_of(raw).instance_of.instance_layout();

            drop_in_place(raw);
            dealloc(raw as *mut u8, layout);
        }
    }

    pub(crate) fn alloc(instance_of: TypePointer) -> ProcessPointer {
        let ptr =
            allocate(unsafe { instance_of.instance_layout() }) as *mut Self;
        let obj = unsafe { &mut *ptr };
        let mut state = ProcessState::new();

        // Processes start without any messages, so we must ensure their status
        // is set accordingly.
        state.status.set_waiting_for_message(true);

        obj.header.init_atomic(instance_of);
        init!(obj.run_lock => UnsafeCell::new(Mutex::new(())));

        // We _must_ set this field explicitly to NULL because
        // Process::next_task depends on being NULL when handling the first
        // message.
        init!(obj.stack_pointer => null_mut());
        init!(obj.state => Mutex::new(state));
        unsafe { ProcessPointer::new(ptr) }
    }

    /// Returns a new Process acting as the main process.
    ///
    /// This process always runs on the main thread.
    pub(crate) fn main(
        instance_of: TypePointer,
        method: NativeAsyncMethod,
    ) -> ProcessPointer {
        let mut process = Self::alloc(instance_of);
        let message = Message { method, data: null_mut() };

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

    pub(crate) fn send_message(
        &mut self,
        message: Message,
    ) -> RescheduleRights {
        let mut state = self.state.lock().unwrap();

        state.mailbox.send(message);
        state.try_reschedule_for_message()
    }

    pub(crate) fn next_task(&mut self, config: &Config) -> Task {
        // We set up the stack here such that the time spent doing so doesn't
        // affect the process/thread that sent us the message or initially
        // spawned the process.
        if self.stack_pointer.is_null() {
            self.initialize_stack(config);
        }

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

        self.stack_pointer = self.stack.stack_pointer();
        state.status.set_running(true);
        Task::Start(message)
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

    pub(crate) fn check_timeout_and_take_poll_bits(&self) -> (bool, u8) {
        let mut state = self.state.lock().unwrap();
        let bits = state.poll_bits;

        state.poll_bits = 0;

        if state.status.timeout_expired() {
            state.status.set_timeout_expired(false);
            (true, bits)
        } else {
            (false, bits)
        }
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, ProcessState> {
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
                    let raw_name = sym_name.as_str().unwrap_or("");

                    // For closures the generated symbol names aren't useful and
                    // subject to change, so we ignore them.
                    if raw_name.starts_with(CLOSURE_IDENTIFIER) {
                        "<closure>".to_string()
                    } else {
                        // We only want to include frames for Inko source code,
                        // not any additional frames introduced by the runtime
                        // library and its dependencies.
                        let base = if let Some(name) =
                            raw_name.strip_prefix(METHOD_IDENTIFIER)
                        {
                            name
                        } else {
                            return;
                        };

                        // Methods include the shape identifiers to prevent name
                        // conflicts. We get rid of these to ensure the
                        // stacktraces are easier to understand.
                        if let Some(idx) = base.find('#') {
                            base[0..idx].to_string()
                        } else {
                            base.to_string()
                        }
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

    fn initialize_stack(&mut self, config: &Config) {
        let stack = Stack::new(config.stack_size as usize, config.page_size);

        // Generated code needs access to the current process. Rust's way of
        // handling thread-locals is such that we can't reliably expose them to
        // generated code. As such, we instead write the necessary data to the
        // start of the stack, which the generated code can then access whenever
        // necessary.
        //
        // Safety: a Process is allocated using `Process::alloc` which always
        // returns a pointer to a stable place in memory. As such it should be
        // safe to treat `self` as a stable pointer here.
        unsafe {
            write(
                stack.private_data_pointer() as *mut StackData,
                StackData {
                    process: ProcessPointer::new(self as *mut _),
                    started_at: 0,
                    thread: null_mut(),
                },
            );
        }

        init!(self.stack_pointer => stack.stack_pointer());
        init!(self.stack => ManuallyDrop::new(stack));
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

    pub(crate) fn start_blocking(mut self) {
        // Safety: threads are stored in processes before running them.
        unsafe { self.thread() }.start_blocking();
    }

    pub(crate) fn stop_blocking(mut self) {
        // Safety: threads are stored in processes before running them.
        unsafe { self.thread() }.stop_blocking(self);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::page_size;
    use crate::test::{empty_process_type, OwnedProcess};
    use std::mem::{offset_of, size_of};
    use std::num::NonZeroU64;

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
        } else {
            assert_eq!(size_of::<UnsafeCell<Mutex<()>>>(), 16);
            assert_eq!(size_of::<Process>(), 120);
        }

        assert_eq!(size_of::<ProcessState>(), 48);
        assert_eq!(size_of::<Option<NonNull<Thread>>>(), 8);
    }

    #[test]
    fn test_field_offsets() {
        assert_eq!(offset_of!(Process, header), 0);
        assert_eq!(
            offset_of!(Process, fields),
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
    fn test_process_status_set_waiting_for_value() {
        let mut status = ProcessStatus::new();

        status.set_waiting_for_value(true);
        assert!(status.is_waiting_for_value());

        status.set_waiting_for_value(false);
        assert!(!status.is_waiting_for_value());
    }

    #[test]
    fn test_process_status_no_longer_waiting() {
        let mut status = ProcessStatus::new();

        status.set_running(true);
        status.set_waiting_for_value(true);
        status.set_waiting_for_io(true);
        status.set_sleeping(true);
        status.no_longer_waiting();

        assert!(status.is_running());
        assert!(!status.is_waiting_for_value());
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
        status.set_waiting_for_value(true);
        assert!(status.is_waiting());

        status.no_longer_waiting();

        assert!(!status.is_waiting_for_value());
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
        assert!(RescheduleRights::AcquiredWithTimeout(TimeoutId(
            NonZeroU64::new(1).unwrap()
        ))
        .are_acquired());
    }

    #[test]
    fn test_process_state_try_reschedule_after_timeout() {
        let mut proc_state = ProcessState::new();

        assert_eq!(
            proc_state.try_reschedule_after_timeout(),
            RescheduleRights::Failed
        );

        proc_state.waiting_for_value(None);

        assert_eq!(
            proc_state.try_reschedule_after_timeout(),
            RescheduleRights::Acquired
        );

        assert!(!proc_state.status.is_waiting_for_value());
        assert!(!proc_state.status.is_waiting());

        let id = TimeoutId(NonZeroU64::new(1).unwrap());

        proc_state.waiting_for_value(Some(id));

        assert_eq!(
            proc_state.try_reschedule_after_timeout(),
            RescheduleRights::AcquiredWithTimeout(id)
        );

        assert!(!proc_state.status.is_waiting_for_value());
        assert!(!proc_state.status.is_waiting());
    }

    #[test]
    fn test_process_state_waiting_for_value() {
        let mut proc_state = ProcessState::new();

        proc_state.waiting_for_value(None);

        assert!(proc_state.status.is_waiting_for_value());
        assert!(proc_state.timeout.is_none());

        proc_state
            .waiting_for_value(Some(TimeoutId(NonZeroU64::new(1).unwrap())));

        assert!(proc_state.status.is_waiting_for_value());
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
    fn test_process_state_try_reschedule_for_value() {
        let mut proc_state = ProcessState::new();

        assert_eq!(
            proc_state.try_reschedule_for_value(),
            RescheduleRights::Failed
        );

        proc_state.status.set_waiting_for_value(true);
        assert_eq!(
            proc_state.try_reschedule_for_value(),
            RescheduleRights::Acquired
        );
        assert!(!proc_state.status.is_waiting_for_value());

        let id = TimeoutId(NonZeroU64::new(1).unwrap());

        proc_state.status.set_waiting_for_value(true);
        proc_state.timeout = Some(id);

        assert_eq!(
            proc_state.try_reschedule_for_value(),
            RescheduleRights::AcquiredWithTimeout(id)
        );
        assert!(!proc_state.status.is_waiting_for_value());
    }

    #[test]
    fn test_process_new() {
        let typ = empty_process_type();
        let process = OwnedProcess::new(Process::alloc(typ.as_pointer()));

        assert_eq!(process.header.instance_of, typ.as_pointer());
    }

    #[test]
    fn test_process_main() {
        let typ = empty_process_type();
        let process =
            OwnedProcess::new(Process::main(typ.as_pointer(), method));

        assert!(process.is_main());
    }

    #[test]
    fn test_process_set_main() {
        let typ = empty_process_type();
        let mut process = OwnedProcess::new(Process::alloc(typ.as_pointer()));

        assert!(!process.is_main());

        process.set_main();
        assert!(process.is_main());
    }

    #[test]
    fn test_process_state_suspend() {
        let typ = empty_process_type();
        let process = OwnedProcess::new(Process::alloc(typ.as_pointer()));

        process.state().suspend(TimeoutId(NonZeroU64::new(1).unwrap()));

        assert!(process.state().timeout.is_some());
        assert!(process.state().status.is_waiting());
    }

    #[test]
    fn test_process_timeout_expired() {
        let typ = empty_process_type();
        let process = OwnedProcess::new(Process::alloc(typ.as_pointer()));

        assert!(!process.timeout_expired());

        process.state().suspend(TimeoutId(NonZeroU64::new(1).unwrap()));

        assert!(!process.timeout_expired());
        assert!(!process.state().status.timeout_expired());
    }

    #[test]
    fn test_process_pointer_identifier() {
        let ptr = unsafe { ProcessPointer::new(0x4 as *mut _) };

        assert_eq!(ptr.identifier(), 0x4);
    }

    #[test]
    fn test_mailbox_new() {
        let mail = Mailbox::new();

        assert_eq!(mail.messages.capacity(), 4);
    }

    #[test]
    fn test_mailbox_send() {
        let mut mail = Mailbox::new();
        let msg = Message { method, data: null_mut() };

        mail.send(msg);
        assert!(mail.receive().is_some());
    }

    #[test]
    fn test_process_send_message() {
        let typ = empty_process_type();
        let mut process = OwnedProcess::new(Process::alloc(typ.as_pointer()));
        let msg = Message { method, data: null_mut() };

        assert_eq!(process.send_message(msg), RescheduleRights::Acquired);
        assert_eq!(process.state().mailbox.messages.len(), 1);
    }

    #[test]
    fn test_process_next_task_without_messages() {
        let typ = empty_process_type();
        let mut process = OwnedProcess::new(Process::alloc(typ.as_pointer()));
        let conf = Config::new();

        assert!(matches!(process.next_task(&conf), Task::Wait));
    }

    #[test]
    fn test_process_next_task_with_new_message() {
        let typ = empty_process_type();
        let mut process = OwnedProcess::new(Process::alloc(typ.as_pointer()));
        let msg = Message { method, data: null_mut() };
        let conf = Config::new();

        process.send_message(msg);
        assert!(matches!(process.next_task(&conf), Task::Start(_)));
    }

    #[test]
    fn test_process_next_task_with_existing_message() {
        let typ = empty_process_type();
        let mut process = OwnedProcess::new(Process::alloc(typ.as_pointer()));
        let msg1 = Message { method, data: null_mut() };
        let msg2 = Message { method, data: null_mut() };
        let conf = Config::new();

        process.send_message(msg1);
        process.next_task(&conf);
        process.send_message(msg2);

        assert!(matches!(process.next_task(&conf), Task::Resume));
    }

    #[test]
    fn test_process_finish_message() {
        let typ = empty_process_type();
        let mut process = OwnedProcess::new(Process::alloc(typ.as_pointer()));

        assert!(!process.finish_message());
        assert!(process.state().status.is_waiting_for_message());
    }
}
