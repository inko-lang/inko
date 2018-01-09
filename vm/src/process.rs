use std::collections::HashSet;
use std::mem;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::cell::UnsafeCell;

use binding::RcBinding;
use block::Block;
use compiled_code::CompiledCodePointer;
use config::Config;
use execution_context::ExecutionContext;
use global_scope::GlobalScopePointer;
use immix::global_allocator::RcGlobalAllocator;
use immix::local_allocator::LocalAllocator;
use mailbox::Mailbox;
use object_pointer::ObjectPointer;
use object_value;
use process_table::PID;

pub type RcProcess = Arc<Process>;

#[derive(Debug)]
pub enum ProcessStatus {
    /// The process has been (re-)scheduled for execution.
    Scheduled,

    /// The process is running.
    Running,

    /// The process has been suspended.
    Suspended,

    /// The process has been suspended for garbage collection.
    SuspendForGc,

    /// The process is waiting for a message to arrive.
    WaitingForMessage,

    /// The process has finished execution.
    Finished,
}

impl ProcessStatus {
    pub fn is_running(&self) -> bool {
        match self {
            &ProcessStatus::Running => true,
            _ => false,
        }
    }
}

pub struct LocalData {
    /// The process-local memory allocator.
    pub allocator: LocalAllocator,

    /// The current execution context of this process.
    pub context: Box<ExecutionContext>,

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

    /// Data stored in a process that should only be modified by a single thread
    /// at once.
    pub local_data: UnsafeCell<LocalData>,
}

unsafe impl Sync for LocalData {}
unsafe impl Send for LocalData {}
unsafe impl Sync for Process {}

impl Process {
    pub fn new(
        pid: PID,
        pool_id: usize,
        context: ExecutionContext,
        global_allocator: RcGlobalAllocator,
    ) -> RcProcess {
        let local_data = LocalData {
            allocator: LocalAllocator::new(global_allocator.clone()),
            context: Box::new(context),
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
            local_data: UnsafeCell::new(local_data),
        };

        Arc::new(process)
    }

    pub fn from_block(
        pid: PID,
        pool_id: usize,
        block: &Block,
        global_allocator: RcGlobalAllocator,
    ) -> RcProcess {
        let context = ExecutionContext::from_isolated_block(block);

        Process::new(pid, pool_id, context, global_allocator)
    }

    pub fn local_data_mut(&self) -> &mut LocalData {
        unsafe { &mut *self.local_data.get() }
    }

    pub fn local_data(&self) -> &LocalData {
        unsafe { &*self.local_data.get() }
    }

    pub fn push_context(&self, context: ExecutionContext) {
        let mut boxed = Box::new(context);
        let local_data = self.local_data_mut();
        let ref mut target = local_data.context;

        mem::swap(target, &mut boxed);

        target.set_parent(boxed);
    }

    pub fn status_integer(&self) -> usize {
        match *lock!(self.status) {
            ProcessStatus::Scheduled => 0,
            ProcessStatus::Running => 1,
            ProcessStatus::Suspended => 2,
            ProcessStatus::SuspendForGc => 3,
            ProcessStatus::WaitingForMessage => 4,
            ProcessStatus::Finished => 5,
        }
    }

    /// Pops an execution context.
    ///
    /// This method returns true if we're at the top of the execution context
    /// stack.
    pub fn pop_context(&self) -> bool {
        let local_data = self.local_data_mut();

        if let Some(parent) = local_data.context.parent.take() {
            local_data.context = parent;

            false
        } else {
            true
        }
    }

    pub fn get_register(&self, register: usize) -> ObjectPointer {
        self.local_data().context.get_register(register)
    }

    pub fn set_register(&self, register: usize, value: ObjectPointer) {
        self.local_data_mut().context.set_register(register, value);
    }

    pub fn set_local(&self, index: usize, value: ObjectPointer) {
        self.local_data_mut().context.set_local(index, value);
    }

    pub fn get_local(&self, index: usize) -> ObjectPointer {
        self.local_data().context.get_local(index)
    }

    pub fn local_exists(&self, index: usize) -> bool {
        let local_data = self.local_data();

        local_data.context.binding.local_exists(index)
    }

    pub fn set_global(&self, index: usize, value: ObjectPointer) {
        self.local_data_mut().context.set_global(index, value);
    }

    pub fn get_global(&self, index: usize) -> ObjectPointer {
        self.local_data().context.get_global(index)
    }

    pub fn allocate_empty(&self) -> ObjectPointer {
        self.local_data_mut().allocator.allocate_empty()
    }

    pub fn allocate(
        &self,
        value: object_value::ObjectValue,
        proto: ObjectPointer,
    ) -> ObjectPointer {
        let local_data = self.local_data_mut();

        local_data.allocator.allocate_with_prototype(value, proto)
    }

    pub fn allocate_without_prototype(
        &self,
        value: object_value::ObjectValue,
    ) -> ObjectPointer {
        let local_data = self.local_data_mut();

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

    pub fn advance_instruction_index(&self) {
        self.local_data_mut().context.instruction_index += 1;
    }

    pub fn binding(&self) -> RcBinding {
        self.context().binding()
    }

    pub fn global_scope(&self) -> &GlobalScopePointer {
        &self.context().global_scope
    }

    pub fn context(&self) -> &Box<ExecutionContext> {
        &self.local_data().context
    }

    pub fn context_mut(&self) -> &mut Box<ExecutionContext> {
        &mut self.local_data_mut().context
    }

    pub fn compiled_code(&self) -> CompiledCodePointer {
        self.context().code.clone()
    }

    pub fn available_for_execution(&self) -> bool {
        match *lock!(self.status) {
            ProcessStatus::Scheduled => true,
            _ => false,
        }
    }

    pub fn set_status(&self, new_status: ProcessStatus) {
        let mut status = lock!(self.status);

        *status = new_status;
    }

    pub fn running(&self) {
        self.set_status(ProcessStatus::Running);
    }

    pub fn finished(&self) {
        self.set_status(ProcessStatus::Finished);
    }

    pub fn scheduled(&self) {
        self.set_status(ProcessStatus::Scheduled);
    }

    pub fn suspended(&self) {
        self.set_status(ProcessStatus::Suspended);
    }

    pub fn suspend_for_gc(&self) {
        self.set_status(ProcessStatus::SuspendForGc);
    }

    pub fn waiting_for_message(&self) {
        self.set_status(ProcessStatus::WaitingForMessage);
    }

    pub fn is_waiting_for_message(&self) -> bool {
        match *lock!(self.status) {
            ProcessStatus::WaitingForMessage => true,
            _ => false,
        }
    }

    pub fn wakeup_after_suspension_timeout(&self) {
        if self.is_waiting_for_message() {
            // When a timeout expires we don't want to retry the last
            // instruction as otherwise we'd end up in an infinite loop if
            // no message is received.
            self.advance_instruction_index();
        }
    }

    pub fn has_messages(&self) -> bool {
        self.local_data().mailbox.has_messages()
    }

    pub fn should_collect_young_generation(&self) -> bool {
        self.local_data().allocator.collect_young
    }

    pub fn should_collect_mature_generation(&self) -> bool {
        self.local_data().allocator.collect_mature
    }

    pub fn should_collect_mailbox(&self) -> bool {
        self.local_data().mailbox.allocator.collect
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
    pub fn write_barrier(
        &self,
        written_to: ObjectPointer,
        written: ObjectPointer,
    ) {
        if written_to.is_mature() && written.is_young() {
            self.local_data_mut().remembered_set.insert(written_to);
        }
    }

    pub fn prepare_for_collection(&self, mature: bool) -> bool {
        self.local_data_mut()
            .allocator
            .prepare_for_collection(mature)
    }

    pub fn reclaim_blocks(&self, mature: bool) {
        self.local_data_mut().allocator.reclaim_blocks(mature);
    }

    pub fn update_collection_statistics(&self, config: &Config, mature: bool) {
        let local_data = self.local_data_mut();

        local_data.allocator.collect_young = false;
        local_data.allocator.collect_mature = false;

        local_data.allocator.increment_young_ages();
        local_data.allocator.young_block_allocations = 0;

        local_data
            .allocator
            .increment_young_threshold(config.young_growth_factor);

        if mature {
            local_data.allocator.mature_block_allocations = 0;

            local_data
                .allocator
                .increment_mature_threshold(config.mature_growth_factor);

            local_data.mature_collections += 1;
        } else {
            local_data.young_collections += 1;
        }
    }

    pub fn update_mailbox_collection_statistics(&self, config: &Config) {
        let local_data = self.local_data_mut();

        local_data.mailbox_collections += 1;

        local_data.mailbox.allocator.collect = false;
        local_data.mailbox.allocator.block_allocations = 0;

        local_data
            .mailbox
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
    use config::Config;
    use vm::test::setup;

    #[test]
    fn test_contexts() {
        let (_machine, _block, process) = setup();

        assert_eq!(process.contexts().len(), 1);
    }

    #[test]
    fn test_update_collection_statistics_without_mature() {
        let (_machine, _block, process) = setup();
        let config = Config::new();

        let old_threshold = process
            .local_data()
            .allocator
            .young_block_allocation_threshold;

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
        let (_machine, _block, process) = setup();
        let config = Config::new();

        {
            let local_data = process.local_data_mut();

            local_data.allocator.young_block_allocations = 1;
            local_data.allocator.mature_block_allocations = 1;
        }

        let old_young_threshold = process
            .local_data()
            .allocator
            .young_block_allocation_threshold;

        let old_mature_threshold = process
            .local_data()
            .allocator
            .mature_block_allocation_threshold;

        process.update_collection_statistics(&config, true);

        let local_data = process.local_data();
        let ref allocator = local_data.allocator;

        assert_eq!(allocator.young_block_allocations, 0);
        assert_eq!(allocator.mature_block_allocations, 0);

        assert_eq!(local_data.young_collections, 0);
        assert_eq!(local_data.mature_collections, 1);

        assert!(
            allocator.young_block_allocation_threshold > old_young_threshold
        );

        assert!(
            allocator.mature_block_allocation_threshold > old_mature_threshold
        );
    }

    #[test]
    fn test_update_mailbox_collection_statistics() {
        let (_machine, _block, process) = setup();
        let config = Config::new();

        {
            let local_data = process.local_data_mut();

            local_data.mailbox.allocator.block_allocations = 1;
        }

        let old_threshold = process
            .local_data()
            .mailbox
            .allocator
            .block_allocation_threshold;

        process.update_mailbox_collection_statistics(&config);

        let local_data = process.local_data();
        let ref mailbox = local_data.mailbox;

        assert_eq!(local_data.mailbox_collections, 1);
        assert_eq!(mailbox.allocator.block_allocations, 0);

        assert!(mailbox.allocator.block_allocation_threshold > old_threshold);
    }
}
