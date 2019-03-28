use arc_without_weak::ArcWithoutWeak;
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
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use object_pointer::ObjectPointer;
use object_value;
use scheduler::pool::Pool;
use scheduler::timeouts::Timeout;
use std::cell::UnsafeCell;
use std::i64;
use std::mem;
use std::ops::Drop;
use std::panic::RefUnwindSafe;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use tagged_pointer::{self, TaggedPointer};
use vm::state::RcState;

pub type RcProcess = ArcWithoutWeak<Process>;

/// The bit that is set to mark a process as being suspended.
const SUSPENDED_BIT: usize = 0;

/// An enum describing what rights a thread was given when trying to reschedule
/// a process.
pub enum RescheduleRights {
    /// The rescheduling rights were not obtained.
    Failed,

    /// The rescheduling rights were obtained.
    Acquired,

    /// The rescheduling rights were obtained, and the process was using a
    /// timeout.
    AcquiredWithTimeout(ArcWithoutWeak<Timeout>),
}

impl RescheduleRights {
    pub fn are_acquired(&self) -> bool {
        match self {
            RescheduleRights::Failed => false,
            _ => true,
        }
    }

    pub fn process_had_timeout(&self) -> bool {
        match self {
            RescheduleRights::AcquiredWithTimeout(_) => true,
            _ => false,
        }
    }
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

    /// A boolean indicating if this process is performing a blocking operation.
    pub blocking: bool,

    /// A boolean indicating if this process is the main process or not.
    ///
    /// When the main process terminates, so does the entire program.
    pub main: bool,

    /// The ID of the thread this process is pinned to.
    pub thread_id: Option<u8>,
}

pub struct Process {
    /// Data stored in a process that should only be modified by a single thread
    /// at once.
    local_data: UnsafeCell<LocalData>,

    /// If the process is waiting for a message.
    waiting_for_message: AtomicBool,

    /// A marker indicating if a process is suspened, optionally including the
    /// pointer to the timeout.
    ///
    /// When this value is NULL, the process is not suspended.
    ///
    /// When the lowest bit is set to 1, the pointer may point to (after
    /// unsetting the bit) to one of the following:
    ///
    /// 1. NULL, meaning the process is suspended indefinitely.
    /// 2. A Timeout, meaning the process is suspended until the timeout
    ///    expires.
    ///
    /// While the type here uses a `TaggedPointer`, in reality the type is an
    /// `ArcWithoutWeak<Timeout>`. This trick is needed to allow for atomic
    /// operations and tagging, something which isn't possible using an
    /// `Option<T>`.
    suspended: TaggedPointer<Timeout>,
}

unsafe impl Sync for LocalData {}
unsafe impl Send for LocalData {}
unsafe impl Sync for Process {}
impl RefUnwindSafe for Process {}

impl Process {
    pub fn with_rc(
        context: ExecutionContext,
        global_allocator: RcGlobalAllocator,
        config: &Config,
    ) -> RcProcess {
        let local_data = LocalData {
            allocator: LocalAllocator::new(global_allocator.clone(), config),
            context: Box::new(context),
            mailbox: Mailbox::new(global_allocator, config),
            panic_handler: ObjectPointer::null(),
            blocking: false,
            main: false,
            thread_id: None,
        };

        ArcWithoutWeak::new(Process {
            local_data: UnsafeCell::new(local_data),
            waiting_for_message: AtomicBool::new(false),
            suspended: TaggedPointer::null(),
        })
    }

    pub fn from_block(
        block: &Block,
        global_allocator: RcGlobalAllocator,
        config: &Config,
    ) -> RcProcess {
        let context = ExecutionContext::from_isolated_block(block);

        Process::with_rc(context, global_allocator, config)
    }

    pub fn set_main(&self) {
        self.local_data_mut().main = true;
    }

    pub fn is_main(&self) -> bool {
        self.local_data().main
    }

    pub fn set_blocking(&self, value: bool) {
        self.local_data_mut().blocking = value;
    }

    pub fn is_blocking(&self) -> bool {
        self.local_data().blocking
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

    pub fn suspend_with_timeout(&self, timeout: ArcWithoutWeak<Timeout>) {
        let pointer = ArcWithoutWeak::into_raw(timeout);
        let tagged = tagged_pointer::with_bit(pointer, SUSPENDED_BIT);

        self.suspended.atomic_store(tagged);
    }

    pub fn suspend_without_timeout(&self) {
        let pointer = ptr::null_mut();
        let tagged = tagged_pointer::with_bit(pointer, SUSPENDED_BIT);

        self.suspended.atomic_store(tagged);
    }

    pub fn is_suspended_with_timeout(
        &self,
        timeout: &ArcWithoutWeak<Timeout>,
    ) -> bool {
        let pointer = self.suspended.atomic_load();

        tagged_pointer::untagged(pointer) == timeout.as_ptr()
    }

    /// Attempts to acquire the rights to reschedule this process.
    pub fn acquire_rescheduling_rights(&self) -> RescheduleRights {
        let current = self.suspended.atomic_load();

        if current.is_null() {
            RescheduleRights::Failed
        } else if self.suspended.compare_and_swap(current, ptr::null_mut()) {
            let untagged = tagged_pointer::untagged(current);

            if untagged.is_null() {
                RescheduleRights::Acquired
            } else {
                let timeout = unsafe { ArcWithoutWeak::from_raw(untagged) };

                RescheduleRights::AcquiredWithTimeout(timeout)
            }
        } else {
            RescheduleRights::Failed
        }
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

    pub fn send_message_from_external_process(&self, message: ObjectPointer) {
        self.local_data_mut().mailbox.send_from_external(message);
    }

    pub fn send_message_from_self(&self, message: ObjectPointer) {
        self.local_data_mut().mailbox.send_from_self(message);
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

    pub fn binding(&self) -> RcBinding {
        self.context().binding()
    }

    pub fn global_scope(&self) -> &GlobalScopePointer {
        &self.context().global_scope
    }

    pub fn context(&self) -> &ExecutionContext {
        &self.local_data().context
    }

    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn context_mut(&self) -> &mut ExecutionContext {
        &mut *self.local_data_mut().context
    }

    pub fn compiled_code(&self) -> CompiledCodePointer {
        self.context().code
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

        let to_finalize = blocks
            .iter_mut()
            .map(|block| {
                block.reset_mark_bitmaps();
                block.prepare_finalization();
                block.reset();

                DerefPointer::new(block)
            })
            .collect::<Vec<_>>();

        if !to_finalize.is_empty() {
            state.finalizer_pool.schedule(to_finalize);
        }

        state.global_allocator.add_blocks(&mut blocks);
    }

    pub fn update_collection_statistics(&self, config: &Config, mature: bool) {
        let local_data = self.local_data_mut();

        local_data
            .allocator
            .update_collection_statistics(config, mature);
    }

    pub fn update_mailbox_collection_statistics(&self, config: &Config) {
        let local_data = self.local_data_mut();

        local_data
            .mailbox
            .allocator
            .update_collection_statistics(config);
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

    pub fn waiting_for_message(&self) {
        self.waiting_for_message.store(true, Ordering::Release);
    }

    pub fn no_longer_waiting_for_message(&self) {
        self.waiting_for_message.store(false, Ordering::Release);
    }

    pub fn is_waiting_for_message(&self) -> bool {
        self.waiting_for_message.load(Ordering::Acquire)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // This ensures the timeout is dropped if it's present, without having
        // to duplicate the dropping logic.
        self.acquire_rescheduling_rights();
    }
}

impl RcProcess {
    /// Returns the unique identifier associated with this process.
    pub fn identifier(&self) -> usize {
        self.as_ptr() as usize
    }
}

impl PartialEq for RcProcess {
    fn eq(&self, other: &Self) -> bool {
        self.identifier() == other.identifier()
    }
}

impl Eq for RcProcess {}

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

        {
            let local_data = process.local_data_mut();

            local_data.allocator.young_config.increment_allocations();
            local_data.allocator.mature_config.increment_allocations();
        }

        process.update_collection_statistics(&machine.state.config, false);

        let local_data = process.local_data();

        assert_eq!(local_data.allocator.young_config.block_allocations, 0);
        assert_eq!(local_data.allocator.mature_config.block_allocations, 1);
    }

    #[test]
    fn test_update_collection_statistics_with_mature() {
        let (machine, _block, process) = setup();

        {
            let local_data = process.local_data_mut();

            local_data.allocator.young_config.increment_allocations();
            local_data.allocator.mature_config.increment_allocations();
        }

        process.update_collection_statistics(&machine.state.config, true);

        let local_data = process.local_data();

        assert_eq!(local_data.allocator.young_config.block_allocations, 0);
        assert_eq!(local_data.allocator.mature_config.block_allocations, 0);
    }

    #[test]
    fn test_update_mailbox_collection_statistics() {
        let (machine, _block, process) = setup();

        process
            .local_data_mut()
            .mailbox
            .allocator
            .config
            .increment_allocations();

        process.update_mailbox_collection_statistics(&machine.state.config);

        let local_data = process.local_data();

        assert_eq!(local_data.mailbox.allocator.config.block_allocations, 0);
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
        assert_eq!(size, 448);
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

    #[test]
    fn test_identifier() {
        let (_machine, _block, process) = setup();

        assert!(process.identifier() > 0);
    }
}
