use std::mem;
use std::rc::Rc;
use std::cell::RefCell;

use call_frame::CallFrame;
use heap::Heap;
use register::Register;
use variable_scope::VariableScope;

/// A mutable, reference counted Thread.
pub type RcThread = Rc<RefCell<Thread>>;

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
pub struct Thread {
    /// The current call frame.
    pub call_frame: CallFrame,

    /// The young heap, most objects will be allocated here.
    pub young_heap: Heap,

    /// The mature heap, used for big objects or those that have outlived
    /// several GC cycles.
    pub mature_heap: Heap
}

impl Thread {
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
    pub fn new(call_frame: CallFrame) -> Thread {
        Thread {
            call_frame: call_frame,
            young_heap: Heap::new(),
            mature_heap: Heap::new()
        }
    }

    /// Creates a new mutable, reference counted Thread.
    pub fn with_rc(call_frame: CallFrame) -> RcThread {
        Rc::new(RefCell::new(Thread::new(call_frame)))
    }

    /// Sets the current CallFrame from a CompiledCode.
    pub fn push_call_frame(&mut self, mut frame: CallFrame) {
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
    pub fn call_frame(&self) -> &CallFrame {
        &self.call_frame
    }

    /// Returns a mutable reference to the current register.
    pub fn register(&mut self) -> &mut Register {
        &mut self.call_frame.register
    }

    /// Returns a mutable reference to the current variable scope.
    pub fn variable_scope(&mut self) -> &mut VariableScope {
        &mut self.call_frame.variables
    }

    /// Returns a mutable reference to the current young heap.
    pub fn young_heap(&mut self) -> &mut Heap {
        &mut self.young_heap
    }

    /// Returns a mutable reference to the current mature heap.
    pub fn mature_heap(&mut self) -> &mut Heap {
        &mut self.mature_heap
    }
}
