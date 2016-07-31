use object_pointer::ObjectPointer;

/// A tuple containing an allocated object pointer and a boolean that indicates
/// whether or not a GC run should be scheduled.
pub type AllocationResult = (ObjectPointer, bool);
