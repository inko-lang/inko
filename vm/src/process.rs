use std::collections::HashSet;
use std::mem;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, Condvar};
use std::cell::UnsafeCell;

use immix::local_allocator::LocalAllocator;
use immix::global_allocator::RcGlobalAllocator;

use binding::RcBinding;
use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use config::Config;
use execution_context::ExecutionContext;
use mailbox::Mailbox;
use object_pointer::ObjectPointer;
use object_value;
use process_table::PID;

pub type RcProcess = Arc<Process>;

#[derive(Debug)]
pub enum ProcessStatus {
    /// The process has been scheduled for execution.
    Scheduled,

    /// The process is running.
    Running,

    /// The process has been suspended.
    Suspended,

    /// The process should be suspended for garbage collection.
    SuspendForGc,

    /// The process has been suspended for garbage collection.
    SuspendedByGc,

    /// The process ran into some kind of error during execution.
    Failed,

    /// The process has finished execution.
    Finished,
}

impl ProcessStatus {
    pub fn is_running(&self) -> bool {
        match *self {
            ProcessStatus::Running => true,
            _ => false,
        }
    }
}

pub enum GcState {
    /// No collector activity is taking place.
    None,

    /// A collection has been scheduled.
    Scheduled,
}

pub struct LocalData {
    /// The process-local memory allocator.
    pub allocator: LocalAllocator,

    /// The current call frame of this process.
    pub call_frame: CallFrame, // TODO: use Box<CallFrame>

    /// The current execution context of this process.
    pub context: Box<ExecutionContext>,

    /// The state of the garbage collector for this process.
    pub gc_state: GcState,

    /// The remembered set of this process. This set is not synchronized via a
    /// lock of sorts. As such the collector must ensure this process is
    /// suspended upon examining the remembered set.
    pub remembered_set: HashSet<ObjectPointer>,

    /// The mailbox for sending/receiving messages.
    ///
    /// The Mailbox is stored in LocalData as a Mailbox uses internal locking
    /// while still allowing a receiver to mutate it without a lock. This means
    /// some operations need a &mut self, which won't be possible if a Mailbox
    /// is stored directly in a Process.
    pub mailbox: Mailbox,

    /// The number of young garbage collections that have been performed.
    pub young_collections: usize,

    /// The number of mature garbage collections that have been performed.
    pub mature_collections: usize,

    /// The number of mailbox collections that have been performed.
    pub mailbox_collections: usize,
}

pub struct Process {
    /// The process identifier of this process.
    pub pid: PID,

    /// The ID of the pool that this process belongs to.
    pub pool_id: usize,

    /// The status of this process.
    pub status: Mutex<ProcessStatus>,

    /// Condition variable used for waking up other threads waiting for this
    /// process' status to change.
    pub status_signaler: Condvar,

    /// Data stored in a process that should only be modified by a single thread
    /// at once.
    pub local_data: UnsafeCell<LocalData>,
}

unsafe impl Sync for LocalData {}
unsafe impl Send for LocalData {}
unsafe impl Sync for Process {}

impl Process {
    pub fn new(pid: PID,
               pool_id: usize,
               call_frame: CallFrame,
               context: ExecutionContext,
               global_allocator: RcGlobalAllocator)
               -> RcProcess {
        let local_data = LocalData {
            allocator: LocalAllocator::new(global_allocator.clone()),
            call_frame: call_frame,
            context: Box::new(context),
            gc_state: GcState::None,
            remembered_set: HashSet::new(),
            mailbox: Mailbox::new(global_allocator),
            young_collections: 0,
            mature_collections: 0,
            mailbox_collections: 0,
        };

        let process = Process {
            pid: pid,
            pool_id: pool_id,
            status: Mutex::new(ProcessStatus::Scheduled),
            status_signaler: Condvar::new(),
            local_data: UnsafeCell::new(local_data),
        };

        Arc::new(process)
    }

    pub fn from_code(pid: PID,
                     pool_id: usize,
                     code: RcCompiledCode,
                     self_obj: ObjectPointer,
                     global_allocator: RcGlobalAllocator)
                     -> RcProcess {
        let frame = CallFrame::from_code(code.clone());
        let context = ExecutionContext::with_object(self_obj, code, None);

        Process::new(pid, pool_id, frame, context, global_allocator)
    }

    pub fn local_data_mut(&self) -> &mut LocalData {
        unsafe { &mut *self.local_data.get() }
    }

    pub fn local_data(&self) -> &LocalData {
        unsafe { &*self.local_data.get() }
    }

    pub fn push_call_frame(&self, mut frame: CallFrame) {
        let mut local_data = self.local_data_mut();
        let ref mut target = local_data.call_frame;

        mem::swap(target, &mut frame);

        target.set_parent(frame);
    }

    pub fn pop_call_frame(&self) {
        let mut local_data = self.local_data_mut();

        if local_data.call_frame.parent.is_none() {
            return;
        }

        let parent = local_data.call_frame.parent.take().unwrap();

        local_data.call_frame = *parent;
    }

    pub fn push_context(&self, context: ExecutionContext) {
        let mut boxed = Box::new(context);
        let mut local_data = self.local_data_mut();
        let ref mut target = local_data.context;

        mem::swap(target, &mut boxed);

        target.set_parent(boxed);
    }

    pub fn pop_context(&self) {
        let mut local_data = self.local_data_mut();

        if local_data.context.parent.is_none() {
            return;
        }

        let parent = local_data.context.parent.take().unwrap();

        local_data.context = parent;
    }

    pub fn get_register(&self, register: usize) -> Result<ObjectPointer, String> {
        self.local_data()
            .context
            .get_register(register)
            .ok_or_else(|| format!("Undefined object in register {}", register))
    }

    pub fn get_register_option(&self, register: usize) -> Option<ObjectPointer> {
        self.local_data().context.get_register(register)
    }

    pub fn set_register(&self, register: usize, value: ObjectPointer) {
        self.local_data_mut().context.set_register(register, value);
    }

    pub fn set_local(&self, index: usize, value: ObjectPointer) {
        self.local_data_mut().context.set_local(index, value);
    }

    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        self.local_data().context.get_local(index)
    }

    pub fn local_exists(&self, index: usize) -> bool {
        let local_data = self.local_data();

        local_data.context.binding.local_exists(index)
    }

    pub fn allocate_empty(&self) -> ObjectPointer {
        self.local_data_mut().allocator.allocate_empty()
    }

    pub fn allocate(&self,
                    value: object_value::ObjectValue,
                    proto: ObjectPointer)
                    -> ObjectPointer {
        let mut local_data = self.local_data_mut();

        local_data.allocator.allocate_with_prototype(value, proto)
    }

    pub fn allocate_without_prototype(&self,
                                      value: object_value::ObjectValue)
                                      -> ObjectPointer {
        let mut local_data = self.local_data_mut();

        local_data.allocator.allocate_without_prototype(value)
    }

    /// Sends a message to the current process.
    pub fn send_message(&self, sender: &RcProcess, message: ObjectPointer) {
        if sender.pid == self.pid {
            self.local_data_mut().mailbox.send_from_self(message);
        } else {
            self.local_data_mut().mailbox.send_from_external(message);
        }
    }

    /// Returns a message from the mailbox.
    pub fn receive_message(&self) -> Option<ObjectPointer> {
        self.local_data_mut().mailbox.receive()
    }

    pub fn should_be_rescheduled(&self) -> bool {
        match *lock!(self.status) {
            ProcessStatus::Suspended => true,
            _ => false,
        }
    }

    /// Adds a new call frame pointing to the given line number.
    pub fn advance_line(&self, line: u32) {
        let frame = CallFrame::new(self.compiled_code(), line);

        self.push_call_frame(frame);
    }

    pub fn binding(&self) -> RcBinding {
        self.context().binding()
    }

    pub fn self_object(&self) -> ObjectPointer {
        self.context().self_object()
    }

    pub fn context(&self) -> &Box<ExecutionContext> {
        &self.local_data().context
    }

    pub fn context_mut(&self) -> &mut Box<ExecutionContext> {
        &mut self.local_data_mut().context
    }

    pub fn at_top_level(&self) -> bool {
        self.context().parent.is_none()
    }

    pub fn call_frame(&self) -> &CallFrame {
        &self.local_data().call_frame
    }

    pub fn compiled_code(&self) -> RcCompiledCode {
        self.context().code.clone()
    }

    pub fn instruction_index(&self) -> usize {
        self.context().instruction_index
    }

    pub fn is_alive(&self) -> bool {
        match *lock!(self.status) {
            ProcessStatus::Failed => false,
            ProcessStatus::Finished => false,
            _ => true,
        }
    }

    pub fn available_for_execution(&self) -> bool {
        match *lock!(self.status) {
            ProcessStatus::Scheduled => true,
            ProcessStatus::Suspended => true,
            _ => false,
        }
    }

    pub fn running(&self) {
        self.set_status(ProcessStatus::Running);
    }

    pub fn set_status(&self, new_status: ProcessStatus) {
        let mut status = lock!(self.status);

        *status = new_status;

        self.status_signaler.notify_all();
    }

    pub fn set_status_without_overwriting_gc_status(&self,
                                                    new_status: ProcessStatus) {
        let mut status = lock!(self.status);

        match *status {
            ProcessStatus::SuspendedByGc |
            ProcessStatus::SuspendForGc => {}
            _ => {
                *status = new_status;
                self.status_signaler.notify_all();
            }
        }
    }

    pub fn finished(&self) {
        self.set_status_without_overwriting_gc_status(ProcessStatus::Finished);
    }

    pub fn suspend(&self) {
        self.set_status_without_overwriting_gc_status(ProcessStatus::Suspended);
    }

    pub fn suspend_for_gc(&self) {
        self.set_status(ProcessStatus::SuspendForGc);
    }

    pub fn suspended_by_gc(&self) -> bool {
        match *lock!(self.status) {
            ProcessStatus::SuspendedByGc => true,
            _ => false,
        }
    }

    pub fn request_gc_suspension(&self) {
        if !self.suspended_by_gc() {
            self.suspend_for_gc();
        }

        self.wait_while_running();
    }

    pub fn wait_while_running(&self) {
        let mut status = lock!(self.status);

        while status.is_running() {
            status = self.status_signaler.wait(status).unwrap();
        }
    }

    pub fn should_suspend_for_gc(&self) -> bool {
        match *lock!(self.status) {
            ProcessStatus::SuspendForGc |
            ProcessStatus::SuspendedByGc => true,
            _ => false,
        }
    }

    pub fn gc_state(&self) -> &GcState {
        &self.local_data().gc_state
    }

    pub fn set_gc_state(&self, new_state: GcState) {
        self.local_data_mut().gc_state = new_state;
    }

    pub fn gc_scheduled(&self) {
        self.set_gc_state(GcState::Scheduled);
    }

    pub fn gc_is_scheduled(&self) -> bool {
        match self.gc_state() {
            &GcState::None => false,
            _ => true,
        }
    }

    pub fn should_collect_young_generation(&self) -> bool {
        self.local_data()
            .allocator
            .young_block_allocation_threshold_exceeded()
    }

    pub fn should_collect_mature_generation(&self) -> bool {
        self.local_data()
            .allocator
            .mature_block_allocation_threshold_exceeded()
    }

    pub fn should_collect_mailbox(&self) -> bool {
        self.local_data()
            .mailbox
            .should_collect()
    }

    pub fn reset_status(&self) {
        self.set_status(ProcessStatus::Scheduled);
        self.set_gc_state(GcState::None);
    }

    pub fn contexts(&self) -> Vec<&ExecutionContext> {
        self.context().contexts().collect()
    }

    pub fn has_remembered_objects(&self) -> bool {
        self.local_data().remembered_set.len() > 0
    }

    /// Write barrier for tracking cross generation writes.
    ///
    /// This barrier is based on the Steele write barrier and tracks the object
    /// that is *written to*, not the object that is being written.
    pub fn write_barrier(&self,
                         written_to: ObjectPointer,
                         written: ObjectPointer) {
        if written_to.is_mature() && written.is_young() {
            self.local_data_mut().remembered_set.insert(written_to);
        }
    }

    pub fn prepare_for_collection(&self, mature: bool) -> bool {
        self.local_data_mut().allocator.prepare_for_collection(mature)
    }

    pub fn reclaim_blocks(&self, mature: bool) {
        self.local_data_mut().allocator.reclaim_blocks(mature);
    }

    pub fn update_collection_statistics(&self, config: &Config, mature: bool) {
        let mut local_data = self.local_data_mut();

        local_data.allocator.increment_young_ages();

        local_data.allocator.young_block_allocations = 0;

        local_data.allocator
            .increment_young_threshold(config.young_growth_factor);

        if mature {
            local_data.allocator.mature_block_allocations = 0;

            local_data.allocator
                .increment_mature_threshold(config.mature_growth_factor);

            local_data.mature_collections += 1;
        } else {
            local_data.young_collections += 1;
        }
    }

    pub fn update_mailbox_collection_statistics(&self, config: &Config) {
        let mut local_data = self.local_data_mut();

        local_data.mailbox_collections += 1;
        local_data.mailbox.allocator.block_allocations = 0;

        local_data.mailbox
            .allocator
            .increment_threshold(config.mailbox_growth_factor);
    }

    pub fn is_main(&self) -> bool {
        self.pid == 0
    }
}

impl PartialEq for Process {
    fn eq(&self, other: &Process) -> bool {
        self.pid == other.pid
    }
}

impl Eq for Process {}

impl Hash for Process {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pid.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;
    use immix::global_allocator::GlobalAllocator;
    use compiled_code::CompiledCode;
    use object_pointer::ObjectPointer;

    fn new_process() -> RcProcess {
        let code = CompiledCode::with_rc("a".to_string(),
                                         "a".to_string(),
                                         1,
                                         Vec::new());

        let self_obj = ObjectPointer::null();

        Process::from_code(1, 0, code, self_obj, GlobalAllocator::new())
    }

    #[test]
    fn test_contexts() {
        let process = new_process();

        assert_eq!(process.contexts().len(), 1);
    }

    #[test]
    fn test_update_collection_statistics_without_mature() {
        let process = new_process();
        let config = Config::new();

        let old_threshold =
            process.local_data().allocator.young_block_allocation_threshold;

        process.local_data_mut().allocator.young_block_allocations = 1;

        process.update_collection_statistics(&config, false);

        let local_data = process.local_data();
        let ref allocator = local_data.allocator;

        assert_eq!(allocator.young_block_allocations, 0);
        assert_eq!(local_data.young_collections, 1);

        assert!(allocator.young_block_allocation_threshold > old_threshold);
    }

    #[test]
    fn test_update_collection_statistics_with_mature() {
        let process = new_process();
        let config = Config::new();

        {
            let mut local_data = process.local_data_mut();

            local_data.allocator.young_block_allocations = 1;
            local_data.allocator.mature_block_allocations = 1;
        }

        let old_young_threshold =
            process.local_data().allocator.young_block_allocation_threshold;

        let old_mature_threshold =
            process.local_data().allocator.mature_block_allocation_threshold;

        process.update_collection_statistics(&config, true);

        let local_data = process.local_data();
        let ref allocator = local_data.allocator;

        assert_eq!(allocator.young_block_allocations, 0);
        assert_eq!(allocator.mature_block_allocations, 0);

        assert_eq!(local_data.young_collections, 0);
        assert_eq!(local_data.mature_collections, 1);

        assert!(allocator.young_block_allocation_threshold > old_young_threshold);

        assert!(allocator.mature_block_allocation_threshold >
                old_mature_threshold);
    }

    #[test]
    fn test_update_mailbox_collection_statistics() {
        let process = new_process();
        let config = Config::new();

        {
            let mut local_data = process.local_data_mut();

            local_data.mailbox.allocator.block_allocations = 1;
        }

        let old_threshold =
            process.local_data().mailbox.allocator.block_allocation_threshold;

        process.update_mailbox_collection_statistics(&config);

        let local_data = process.local_data();
        let ref mailbox = local_data.mailbox;

        assert_eq!(local_data.mailbox_collections, 1);
        assert_eq!(mailbox.allocator.block_allocations, 0);

        assert!(mailbox.allocator.block_allocation_threshold > old_threshold);
    }
}
