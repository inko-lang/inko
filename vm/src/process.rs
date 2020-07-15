use crate::arc_without_weak::ArcWithoutWeak;
use crate::block::Block;
use crate::config::Config;
use crate::execution_context::ExecutionContext;
use crate::immix::block_list::BlockList;
use crate::immix::copy_object::CopyObject;
use crate::immix::global_allocator::RcGlobalAllocator;
use crate::immix::local_allocator::LocalAllocator;
use crate::mailbox::Mailbox;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use crate::object_value;
use crate::process_status::ProcessStatus;
use crate::scheduler::timeouts::Timeout;
use crate::tagged_pointer::{self, TaggedPointer};
use crate::vm::state::State;
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use parking_lot::Mutex;
use std::cell::UnsafeCell;
use std::i64;
use std::mem;
use std::ops::Drop;
use std::panic::RefUnwindSafe;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

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

    /// The mailbox of this process.
    ///
    /// We store this in LocalData so that we can borrow fields from LocalData
    /// while also borrowing the mailbox.
    pub mailbox: Mutex<Mailbox>,

    // A block to execute in the event of a panic.
    pub panic_handler: ObjectPointer,

    /// The current execution context of this process.
    pub context: Box<ExecutionContext>,

    /// The ID of the thread this process is pinned to.
    pub thread_id: Option<u8>,

    /// The status of the process.
    status: ProcessStatus,

    /// The result produced by a method call, throw, or another instruction that
    /// may trigger the unwinding of call frames.
    ///
    /// This data is saved on a per-process basis, as processes may be suspended
    /// between a return and the use of this value.
    result: ObjectPointer,
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
            allocator: LocalAllocator::new(global_allocator, config),
            context: Box::new(context),
            panic_handler: ObjectPointer::null(),
            thread_id: None,
            mailbox: Mutex::new(Mailbox::new()),
            status: ProcessStatus::new(),
            result: ObjectPointer::null(),
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
        self.local_data_mut().status.set_main();
    }

    pub fn is_main(&self) -> bool {
        self.local_data().status.is_main()
    }

    pub fn set_blocking(&self, enable: bool) {
        self.local_data_mut().status.set_blocking(enable);
    }

    pub fn is_blocking(&self) -> bool {
        self.local_data().status.is_blocking()
    }

    pub fn set_terminated(&self) {
        self.local_data_mut().status.set_terminated();
    }

    pub fn is_terminated(&self) -> bool {
        self.local_data().status.is_terminated()
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

    pub fn send_message_from_external_process(
        &self,
        message_to_copy: ObjectPointer,
    ) {
        let local_data = self.local_data_mut();

        // The lock must be acquired first, as the receiving process may be
        // garbage collected at this time.
        let mut mailbox = local_data.mailbox.lock();

        // When a process terminates it will acquire the mailbox lock first.
        // Checking the status after acquiring the lock allows us to obtain a
        // stable view of the status.
        if self.is_terminated() {
            return;
        }

        mailbox.send(local_data.allocator.copy_object(message_to_copy));
    }

    pub fn send_message_from_self(&self, message: ObjectPointer) {
        self.local_data_mut().mailbox.lock().send(message);
    }

    pub fn receive_message(&self) -> Option<ObjectPointer> {
        self.local_data_mut().mailbox.lock().receive()
    }

    pub fn context(&self) -> &ExecutionContext {
        &self.local_data().context
    }

    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn context_mut(&self) -> &mut ExecutionContext {
        &mut *self.local_data_mut().context
    }

    pub fn has_messages(&self) -> bool {
        self.local_data().mailbox.lock().has_messages()
    }

    pub fn should_collect_young_generation(&self) -> bool {
        self.local_data().allocator.should_collect_young()
    }

    pub fn should_collect_mature_generation(&self) -> bool {
        self.local_data().allocator.should_collect_mature()
    }

    pub fn contexts(&self) -> Vec<&ExecutionContext> {
        self.context().contexts().collect()
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

    pub fn reclaim_blocks(&self, state: &State, mature: bool) {
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

        blocks
    }

    pub fn terminate(&self, state: &State) {
        // The mailbox lock _must_ be acquired first, otherwise we may end up
        // reclaiming blocks while another process is allocating message into
        // them.
        let _mailbox = self.local_data_mut().mailbox.lock();
        let mut blocks = self.reclaim_all_blocks();

        // Once terminated we don't want to receive any messages any more, as
        // they will never be received and thus lead to an increase in memory.
        // Thus, we mark the process as terminated. We must do this _after_
        // acquiring the lock to ensure other processes sending messages will
        // observe the right value.
        self.set_terminated();

        for block in blocks.iter_mut() {
            block.reset();
            block.finalize();
        }

        state.global_allocator.add_blocks(&mut blocks);
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

    pub fn each_global_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        if let Some(handler) = self.panic_handler() {
            callback(handler.pointer());
        }

        let local_data = self.local_data_mut();

        if !local_data.result.is_null() {
            callback(local_data.result.pointer());
        }
    }

    pub fn each_remembered_pointer<F>(&self, callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        self.local_data_mut()
            .allocator
            .each_remembered_pointer(callback);
    }

    pub fn prune_remembered_set(&self) {
        self.local_data_mut().allocator.prune_remembered_objects();
    }

    pub fn remember_object(&self, pointer: ObjectPointer) {
        self.local_data_mut().allocator.remember_object(pointer);
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

    pub fn set_result(&self, result: ObjectPointer) {
        self.local_data_mut().result = result;
    }

    pub fn take_result(&self) -> ObjectPointer {
        let mut local_data = self.local_data_mut();
        let res = local_data.result;

        local_data.result = ObjectPointer::null();

        res
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
    use crate::object_value;
    use crate::vm::test::setup;
    use num_bigint::BigInt;
    use std::f64;
    use std::i32;
    use std::i64;
    use std::mem;

    #[test]
    fn test_contexts() {
        let (_machine, _block, process) = setup();

        assert_eq!(process.contexts().len(), 1);
    }

    #[test]
    fn test_reclaim_blocks_without_mature() {
        let (machine, _block, process) = setup();

        {
            let local_data = process.local_data_mut();

            local_data.allocator.young_config.increment_allocations();
            local_data.allocator.mature_config.increment_allocations();
        }

        process.reclaim_blocks(&machine.state, false);

        let local_data = process.local_data();

        assert_eq!(local_data.allocator.young_config.block_allocations, 0);
        assert_eq!(local_data.allocator.mature_config.block_allocations, 1);
    }

    #[test]
    fn test_reclaim_blocks_with_mature() {
        let (machine, _block, process) = setup();

        {
            let local_data = process.local_data_mut();

            local_data.allocator.young_config.increment_allocations();
            local_data.allocator.mature_config.increment_allocations();
        }

        process.reclaim_blocks(&machine.state, true);

        let local_data = process.local_data();

        assert_eq!(local_data.allocator.young_config.block_allocations, 0);
        assert_eq!(local_data.allocator.mature_config.block_allocations, 0);
    }

    #[test]
    fn test_receive_message() {
        let (machine, _block, process) = setup();

        let input_message = process
            .allocate(object_value::integer(14), process.allocate_empty());

        let attr = machine.state.intern_string("hello".to_string());

        input_message.add_attribute(&process, attr, attr);

        process.send_message_from_external_process(input_message);

        let received = process.receive_message().unwrap();

        assert!(received.is_young());
        assert!(received.get().value.is_integer());
        assert!(received.get().prototype().is_some());
        assert!(received.get().attributes_map().is_some());
        assert!(received.is_finalizable());
        assert!(received.raw.raw != input_message.raw.raw);
    }

    #[test]
    fn test_send_message_from_external_process_with_closed_mailbox() {
        let (_machine, _block, process) = setup();

        let message = process
            .allocate(object_value::integer(14), process.allocate_empty());

        process.set_terminated();
        process.send_message_from_external_process(message);

        assert!(process.receive_message().is_none());
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
        // This test is put in place to ensure the type size doesn't change
        // unintentionally.
        assert_eq!(mem::size_of::<Process>(), 368);
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

    #[test]
    fn test_each_global_pointer() {
        let (_machine, _block, process) = setup();

        process.set_panic_handler(ObjectPointer::integer(5));
        process.set_result(ObjectPointer::integer(7));

        let mut pointers = Vec::new();

        process.each_global_pointer(|ptr| pointers.push(ptr));

        assert_eq!(pointers.len(), 2);
    }
}
