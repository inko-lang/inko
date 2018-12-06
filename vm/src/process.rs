use num_bigint::BigInt;
use num_traits::FromPrimitive;
use std::cell::UnsafeCell;
use std::hash::{Hash, Hasher};
use std::i64;
use std::mem;
use std::panic::RefUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use binding::RcBinding;
use block::Block;
use compiled_code::CompiledCodePointer;
use config::Config;
use deref_pointer::DerefPointer;
use execution_context::ExecutionContext;
use gc::work_list::WorkList;
use global_scope::GlobalScopePointer;
use immix::block_list::BlockList;
use immix::copy_object::CopyObject;
use immix::global_allocator::RcGlobalAllocator;
use immix::local_allocator::LocalAllocator;
use mailbox::Mailbox;
use object_pointer::ObjectPointer;
use object_value;
use pool::Job;
use process_table::PID;
use vm::state::RcState;

pub type RcProcess = Arc<Process>;

#[derive(Debug, Clone, Copy)]
#[repr(usize)]
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

pub struct LocalData {
    /// The process-local memory allocator.
    pub allocator: LocalAllocator,

    /// The mailbox for sending/receiving messages.
    ///
    /// The Mailbox is stored in LocalData as a Mailbox uses internal locking
    /// while still allowing a receiver to mutate it without a lock. This means
    /// some operations need a &mut self, which won't be possible if a Mailbox
    /// is stored directly in a Process.
    pub mailbox: Mailbox,

    // A block to execute in the event of a panic.
    pub panic_handler: ObjectPointer,

    /// The current execution context of this process.
    pub context: Box<ExecutionContext>,

    /// The number of young garbage collections that have been performed.
    pub young_collections: u16,

    /// The number of mature garbage collections that have been performed.
    pub mature_collections: u16,

    /// The number of mailbox collections that have been performed.
    pub mailbox_collections: u16,

    /// The ID of the pool that this process belongs to.
    pub pool_id: u8,

    /// The ID of the thread this process is pinned to.
    pub thread_id: Option<u8>,
}

pub struct Process {
    /// Data stored in a process that should only be modified by a single thread
    /// at once.
    pub local_data: UnsafeCell<LocalData>,

    /// The process identifier of this process.
    pub pid: PID,

    /// The status of this process.
    pub status: AtomicUsize,
}

unsafe impl Sync for LocalData {}
unsafe impl Send for LocalData {}
unsafe impl Sync for Process {}
impl RefUnwindSafe for Process {}

impl Process {
    pub fn new(
        pid: PID,
        pool_id: u8,
        context: ExecutionContext,
        global_allocator: RcGlobalAllocator,
        config: &Config,
    ) -> RcProcess {
        let local_data = LocalData {
            allocator: LocalAllocator::new(global_allocator.clone(), config),
            context: Box::new(context),
            mailbox: Mailbox::new(global_allocator, config),
            young_collections: 0,
            mature_collections: 0,
            mailbox_collections: 0,
            panic_handler: ObjectPointer::null(),
            pool_id,
            thread_id: None,
        };

        let process = Process {
            pid,
            status: AtomicUsize::new(ProcessStatus::Scheduled as usize),
            local_data: UnsafeCell::new(local_data),
        };

        Arc::new(process)
    }

    pub fn from_block(
        pid: PID,
        pool_id: u8,
        block: &Block,
        global_allocator: RcGlobalAllocator,
        config: &Config,
    ) -> RcProcess {
        let context = ExecutionContext::from_isolated_block(block);

        Process::new(pid, pool_id, context, global_allocator, config)
    }

    pub fn set_pool_id(&self, id: u8) {
        self.local_data_mut().pool_id = id;
    }

    pub fn pool_id(&self) -> u8 {
        self.local_data().pool_id
    }

    pub fn thread_id(&self) -> Option<u8> {
        self.local_data().thread_id
    }

    pub fn set_thread_id(&self, id: u8) {
        self.local_data_mut().thread_id = Some(id);
    }

    pub fn unset_thread_id(&self) {
        self.local_data_mut().thread_id = None;
    }

    pub fn is_pinned(&self) -> bool {
        self.thread_id().is_some()
    }

    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn local_data_mut(&self) -> &mut LocalData {
        unsafe { &mut *self.local_data.get() }
    }

    pub fn local_data(&self) -> &LocalData {
        unsafe { &*self.local_data.get() }
    }

    pub fn push_context(&self, context: ExecutionContext) {
        let mut boxed = Box::new(context);
        let local_data = self.local_data_mut();
        let target = &mut local_data.context;

        mem::swap(target, &mut boxed);

        target.set_parent(boxed);
    }

    pub fn status_integer(&self) -> u8 {
        self.status() as u8
    }

    pub fn status(&self) -> ProcessStatus {
        let status = self.status.load(Ordering::Acquire);

        match status {
            0 => ProcessStatus::Scheduled,
            1 => ProcessStatus::Running,
            2 => ProcessStatus::Suspended,
            3 => ProcessStatus::SuspendForGc,
            4 => ProcessStatus::WaitingForMessage,
            5 => ProcessStatus::Finished,
            _ => panic!("invalid process status: {}", status),
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

    pub fn allocate_usize(
        &self,
        value: usize,
        prototype: ObjectPointer,
    ) -> ObjectPointer {
        self.allocate_u64(value as u64, prototype)
    }

    pub fn allocate_i64(
        &self,
        value: i64,
        prototype: ObjectPointer,
    ) -> ObjectPointer {
        if ObjectPointer::integer_too_large(value) {
            self.allocate(object_value::integer(value), prototype)
        } else {
            ObjectPointer::integer(value)
        }
    }

    pub fn allocate_u64(
        &self,
        value: u64,
        prototype: ObjectPointer,
    ) -> ObjectPointer {
        if ObjectPointer::unsigned_integer_as_big_integer(value) {
            // The value is too large to fit in a i64, so we need to allocate it
            // as a big integer.
            self.allocate(object_value::bigint(BigInt::from(value)), prototype)
        } else if ObjectPointer::unsigned_integer_too_large(value) {
            // The value is small enough that it can fit in an i64, but not
            // small enough that it can fit in a _tagged_ i64.
            self.allocate(object_value::integer(value as i64), prototype)
        } else {
            ObjectPointer::integer(value as i64)
        }
    }

    pub fn allocate_f64_as_i64(
        &self,
        value: f64,
        prototype: ObjectPointer,
    ) -> Result<ObjectPointer, String> {
        if value.is_nan() {
            return Err("A NaN can not be converted to an Integer".to_string());
        } else if value.is_infinite() {
            return Err("An infinite Float can not be converted to an Integer"
                .to_string());
        }

        // We use >= and <= here, as i64::MAX as a f64 can't be casted back to
        // i64, since `i64::MAX as f64` will produce a value slightly larger
        // than `i64::MAX`.
        let pointer = if value >= i64::MAX as f64 || value <= i64::MIN as f64 {
            self.allocate(
                object_value::bigint(BigInt::from_f64(value).unwrap()),
                prototype,
            )
        } else {
            self.allocate_i64(value as i64, prototype)
        };

        Ok(pointer)
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
        let local_data = self.local_data_mut();
        let (should_copy, pointer_opt) = local_data.mailbox.receive();

        if let Some(mailbox_pointer) = pointer_opt {
            let pointer = if should_copy {
                // When another process sends us a message, the message will be
                // copied onto the mailbox heap. We can't directly use such a
                // pointer, as it might be garbage collected when it no longer
                // resides in the mailbox (e.g. after a receive).
                //
                // To work around this, we move the data from the mailbox heap
                // into the process' local heap.
                local_data.allocator.move_object(mailbox_pointer)
            } else {
                mailbox_pointer
            };

            Some(pointer)
        } else {
            None
        }
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

    #[cfg_attr(feature = "cargo-clippy", allow(borrowed_box))]
    pub fn context(&self) -> &Box<ExecutionContext> {
        &self.local_data().context
    }

    #[cfg_attr(feature = "cargo-clippy", allow(borrowed_box, mut_from_ref))]
    pub fn context_mut(&self) -> &mut ExecutionContext {
        &mut *self.local_data_mut().context
    }

    pub fn compiled_code(&self) -> CompiledCodePointer {
        self.context().code
    }

    pub fn available_for_execution(&self) -> bool {
        match self.status() {
            ProcessStatus::Scheduled => true,
            _ => false,
        }
    }

    pub fn set_status(&self, new_status: ProcessStatus) {
        self.status.store(new_status as usize, Ordering::Release);
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
        match self.status() {
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
        self.local_data().allocator.should_collect_young()
    }

    pub fn should_collect_mature_generation(&self) -> bool {
        self.local_data().allocator.should_collect_mature()
    }

    pub fn should_collect_mailbox(&self) -> bool {
        self.local_data().mailbox.allocator.should_collect()
    }

    pub fn contexts(&self) -> Vec<&ExecutionContext> {
        self.context().contexts().collect()
    }

    pub fn has_remembered_objects(&self) -> bool {
        self.local_data().allocator.has_remembered_objects()
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
            self.local_data_mut().allocator.remember_object(written_to);
        }
    }

    pub fn prepare_for_collection(&self, mature: bool) -> bool {
        self.local_data_mut()
            .allocator
            .prepare_for_collection(mature)
    }

    pub fn reclaim_blocks(&self, state: &RcState, mature: bool) {
        self.local_data_mut()
            .allocator
            .reclaim_blocks(state, mature);
    }

    pub fn reclaim_all_blocks(&self) -> BlockList {
        let local_data = self.local_data_mut();
        let mut blocks = BlockList::new();

        for bucket in &mut local_data.allocator.young_generation {
            blocks.append(&mut bucket.blocks);
        }

        blocks.append(&mut local_data.allocator.mature_generation.blocks);
        blocks.append(&mut local_data.mailbox.allocator.bucket.blocks);

        blocks
    }

    pub fn reclaim_and_finalize(&self, state: &RcState) {
        let mut blocks = self.reclaim_all_blocks();

        for block in blocks.iter_mut() {
            block.reset_mark_bitmaps();
            block.prepare_finalization();
            block.reset();

            state
                .finalizer_pool
                .schedule(Job::normal(DerefPointer::new(block)));
        }

        state.global_allocator.add_blocks(&mut blocks);
    }

    pub fn update_collection_statistics(&self, config: &Config, mature: bool) {
        let local_data = self.local_data_mut();

        local_data.allocator.update_collection_statistics(config);

        if mature {
            local_data.mature_collections += 1;
        } else {
            local_data.young_collections += 1;
        }
    }

    pub fn update_mailbox_collection_statistics(&self, config: &Config) {
        let local_data = self.local_data_mut();

        local_data.mailbox_collections += 1;
        local_data
            .mailbox
            .allocator
            .update_collection_statistics(config);
    }

    pub fn is_main(&self) -> bool {
        self.pid == 0
    }

    pub fn panic_handler(&self) -> Option<&ObjectPointer> {
        let local_data = self.local_data();

        if local_data.panic_handler.is_null() {
            None
        } else {
            Some(&local_data.panic_handler)
        }
    }

    pub fn set_panic_handler(&self, handler: ObjectPointer) {
        self.local_data_mut().panic_handler = handler;
    }

    pub fn global_pointers_to_trace(&self) -> WorkList {
        let mut pointers = WorkList::new();

        if let Some(handler) = self.panic_handler() {
            pointers.push(handler.pointer());
        }

        pointers
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
    use num_bigint::BigInt;
    use object_value;
    use std::f64;
    use std::i32;
    use std::i64;
    use std::mem;
    use vm::test::setup;

    #[test]
    fn test_contexts() {
        let (_machine, _block, process) = setup();

        assert_eq!(process.contexts().len(), 1);
    }

    #[test]
    fn test_update_collection_statistics_without_mature() {
        let (machine, _block, process) = setup();

        process.update_collection_statistics(&machine.state.config, false);

        let local_data = process.local_data();

        assert_eq!(local_data.young_collections, 1);
    }

    #[test]
    fn test_update_collection_statistics_with_mature() {
        let (machine, _block, process) = setup();

        process.update_collection_statistics(&machine.state.config, true);

        let local_data = process.local_data();

        assert_eq!(local_data.young_collections, 0);
        assert_eq!(local_data.mature_collections, 1);
    }

    #[test]
    fn test_update_mailbox_collection_statistics() {
        let (machine, _block, process) = setup();

        process.update_mailbox_collection_statistics(&machine.state.config);

        let local_data = process.local_data();

        assert_eq!(local_data.mailbox_collections, 1);
    }

    #[test]
    fn test_receive_message() {
        let (machine, _block, process) = setup();

        // Simulate sending a message from an external process.
        let input_message = process
            .allocate(object_value::integer(14), process.allocate_empty());

        let attr = machine.state.intern_string("hello".to_string());

        input_message.add_attribute(&process, attr, attr);

        process
            .local_data_mut()
            .mailbox
            .send_from_external(input_message);

        let received = process.receive_message().unwrap();

        assert!(received.is_young());
        assert!(received.get().value.is_integer());
        assert!(received.get().prototype().is_some());
        assert!(received.get().attributes_map().is_some());
        assert!(received.is_finalizable());
    }

    #[test]
    fn test_allocate_f64_as_i64_with_a_small_float() {
        let (machine, _block, process) = setup();

        let result =
            process.allocate_f64_as_i64(1.5, machine.state.integer_prototype);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().integer_value().unwrap(), 1);
    }

    #[test]
    fn test_allocate_f64_as_i64_with_a_medium_float() {
        let (machine, _block, process) = setup();

        let float = i32::MAX as f64;
        let result =
            process.allocate_f64_as_i64(float, machine.state.integer_prototype);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().integer_value().unwrap(), i32::MAX as i64);
    }

    #[test]
    fn test_allocate_f64_as_i64_with_a_large_float() {
        let (machine, _block, process) = setup();

        let float = i64::MAX as f64;
        let result =
            process.allocate_f64_as_i64(float, machine.state.integer_prototype);

        let max = BigInt::from(i64::MAX);

        assert!(result.is_ok());
        assert!(result.unwrap().bigint_value().unwrap() >= &max);
    }

    #[test]
    fn test_allocate_f64_as_i64_with_a_nan() {
        let (machine, _block, process) = setup();
        let result = process
            .allocate_f64_as_i64(f64::NAN, machine.state.integer_prototype);

        assert!(result.is_err());
    }

    #[test]
    fn test_allocate_f64_as_i64_with_infinity() {
        let (machine, _block, process) = setup();
        let result = process.allocate_f64_as_i64(
            f64::INFINITY,
            machine.state.integer_prototype,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_allocate_f64_as_i64_with_negative_infinity() {
        let (machine, _block, process) = setup();
        let result = process.allocate_f64_as_i64(
            f64::NEG_INFINITY,
            machine.state.integer_prototype,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_process_type_size() {
        let size = mem::size_of::<Process>();

        // This test is put in place to ensure the type size doesn't change
        // unintentionally.
        assert_eq!(size, 528);
    }

    #[test]
    fn test_process_set_thread_id() {
        let (_machine, _block, process) = setup();

        assert!(process.thread_id().is_none());

        process.set_thread_id(4);

        assert_eq!(process.thread_id(), Some(4));

        process.unset_thread_id();

        assert!(process.thread_id().is_none());
    }
}
