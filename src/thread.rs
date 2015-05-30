use std::mem;

use call_frame::CallFrame;
use compiled_code::CompiledCode;
use heap::Heap;
use register::Register;
use variable_scope::VariableScope;

/// Struct representing a VM thread.
///
/// The Thread struct represents a VM thread which in turn can be mapped to an
/// actual thread, although this is technically not required. Note that these
/// are _not_ green threads, instead the VM uses regular threads and creates a
/// new Thread struct for every OS thread.
///
/// The Thread struct stores information such as the current call frame, the
/// young/mature heaps and providers various convenience methods for allocating
/// objects and working with registers.
///
pub struct Thread<'l> {
    /// The current call frame.
    pub call_frame: CallFrame<'l>,

    /// The young heap, most objects will be allocated here.
    pub young_heap: Heap<'l>,

    /// The mature heap, used for big objects or those that have outlived
    /// several GC cycles.
    pub mature_heap: Heap<'l>
}

impl<'l> Thread<'l> {
    /// Creates a new Thread.
    ///
    /// This does _not_ start an actual OS thread as this is handled by the VM
    /// itself. Creating a thread requires an already existing CallFrame.
    ///
    /// # Examples
    ///
    ///     let frame  = CallFrame::new(...);
    ///     let thread = Thread::new(frame);
    ///
    pub fn new(call_frame: CallFrame<'l>) -> Thread<'l> {
        Thread {
            call_frame: call_frame,
            young_heap: Heap::new(),
            mature_heap: Heap::new()
        }
    }

    /// Sets the current CallFrame from a CompiledCode.
    pub fn add_call_frame_from_compiled_code(&mut self, code: &CompiledCode) {
        let mut frame = code.new_call_frame();

        mem::swap(&mut self.call_frame, &mut frame);

        self.call_frame.set_parent(frame);
    }

    /// Switches the current call frame to the previous one.
    pub fn pop_call_frame(&mut self) {
        let parent = self.call_frame.parent.take().unwrap();

        // TODO: this might move the data from heap back to the stack?
        self.call_frame = *parent;
    }

    /// Returns a reference to the current call frame.
    pub fn call_frame(&self) -> &CallFrame<'l> {
        &self.call_frame
    }

    /// Returns a mutable reference to the current register.
    pub fn register(&mut self) -> &mut Register<'l> {
        &mut self.call_frame.register
    }

    /// Returns a mutable reference to the current variable scope.
    pub fn variable_scope(&mut self) -> &mut VariableScope<'l> {
        &mut self.call_frame.variables
    }

    /// Returns a mutable reference to the current young heap.
    pub fn young_heap(&mut self) -> &mut Heap<'l> {
        &mut self.young_heap
    }

    /// Returns a mutable reference to the current mature heap.
    pub fn mature_heap(&mut self) -> &mut Heap<'l> {
        &mut self.mature_heap
    }
}
