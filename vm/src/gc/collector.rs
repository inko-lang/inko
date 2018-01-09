//! Functions and macros for performing garbage collection.
use gc::trace_result::TraceResult;

use process::RcProcess;
use object_pointer::{ObjectPointer, ObjectPointerPointer};
use object::ObjectStatus;

/// Macro used for conditionally moving objects or resolving forwarding
/// pointers.
macro_rules! move_object {
    ($bucket: expr, $pointer: expr, $status: ident, $body: expr) => ({
        let lock = $bucket.lock();

        match $pointer.status() {
            ObjectStatus::Resolve => $pointer.resolve_forwarding_pointer(),
            ObjectStatus::$status => $body,
            _ => {}
        }

        // Let's explicitly drop the lock for good measurement.
        drop(lock);
    });
}

/// Macro that returns true if the pointer can be skipped during tracing.
macro_rules! can_skip_pointer {
    ($pointer: expr, $mature: expr) =>
        ( $pointer.is_marked() || !$mature && $pointer.is_mature() );
}

/// Promotes an object to the mature generation.
///
/// The pointer to promote is updated to point to the new location.
pub fn promote_mature(process: &RcProcess, pointer: &mut ObjectPointer) {
    pointer.unmark_for_finalization();

    let local_data = process.local_data_mut();
    let old_obj = pointer.get_mut();
    let new_pointer = local_data.allocator.allocate_mature(old_obj.take());

    old_obj.forward_to(new_pointer);

    pointer.resolve_forwarding_pointer();
}

// Evacuates a pointer.
//
// The pointer to evacuate is updated to point to the new location.
pub fn evacuate(process: &RcProcess, pointer: &mut ObjectPointer) {
    pointer.unmark_for_finalization();

    // When evacuating an object we must ensure we evacuate the object into
    // the same bucket.
    let local_data = process.local_data_mut();
    let bucket = pointer.block_mut().bucket_mut().unwrap();

    let old_obj = pointer.get_mut();
    let new_obj = old_obj.take();

    let (_, new_pointer) =
        bucket.allocate(&local_data.allocator.global_allocator, new_obj);

    old_obj.forward_to(new_pointer);

    pointer.resolve_forwarding_pointer();
}

/// Traces through the given pointers, and potentially moves objects around.
pub fn trace_pointers_with_moving(
    process: &RcProcess,
    mut objects: Vec<ObjectPointerPointer>,
    mature: bool,
) -> TraceResult {
    let local_data = process.local_data();
    let ref allocator = local_data.allocator;
    let mut marked = 0;
    let mut evacuated = 0;
    let mut promoted = 0;

    while let Some(pointer_pointer) = objects.pop() {
        let pointer = pointer_pointer.get_mut();

        if can_skip_pointer!(pointer, mature) {
            continue;
        }

        match pointer.status() {
            ObjectStatus::Resolve => pointer.resolve_forwarding_pointer(),
            ObjectStatus::Promote => {
                let ref bucket = allocator.mature_generation;

                move_object!(bucket, pointer, Promote, {
                    promote_mature(process, pointer);

                    promoted += 1;
                });
            }
            ObjectStatus::Evacuate => {
                // To prevent borrow problems we first acquire a new
                // reference to the pointer before locking its
                // bucket.
                let bucket = pointer_pointer.get().block().bucket().unwrap();

                move_object!(bucket, pointer, Evacuate, {
                    evacuate(process, pointer);

                    evacuated += 1;
                });
            }
            _ => {}
        }

        pointer.mark();

        marked += 1;

        pointer.get().push_pointers(&mut objects);
    }

    TraceResult::with(marked, evacuated, promoted)
}

/// Traces through the roots and all their child pointers, without moving
/// objects around.
pub fn trace_pointers_without_moving(
    mut objects: Vec<ObjectPointerPointer>,
    mature: bool,
) -> TraceResult {
    let mut marked = 0;

    while let Some(pointer_pointer) = objects.pop() {
        let pointer = pointer_pointer.get();

        if can_skip_pointer!(pointer, mature) {
            continue;
        }

        pointer.mark();

        marked += 1;

        pointer.get().push_pointers(&mut objects);
    }

    TraceResult::with(marked, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::Object;
    use object_value;
    use vm::test::setup;

    #[test]
    fn test_promote_mature() {
        let (_machine, _block, process) = setup();

        let mut pointer =
            process.allocate_without_prototype(object_value::float(15.0));

        let old_address = pointer.raw.raw as usize;

        promote_mature(&process, &mut pointer);

        let new_address = pointer.raw.raw as usize;

        assert!(old_address != new_address);
        assert!(pointer.is_mature());
        assert_eq!(pointer.float_value().unwrap(), 15.0);
    }

    #[test]
    fn test_evacuate() {
        let (_machine, _block, process) = setup();

        let mut pointer =
            process.allocate_without_prototype(object_value::float(15.0));

        let old_address = pointer.raw.raw as usize;

        evacuate(&process, &mut pointer);

        let new_address = pointer.raw.raw as usize;

        assert!(old_address != new_address);
        assert_eq!(pointer.float_value().unwrap(), 15.0);
    }

    #[test]
    fn test_trace_pointers_with_moving_without_mature() {
        let (_machine, _block, process) = setup();

        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent
            .get_mut()
            .add_attribute(young_child, young_child);

        young_parent.block_mut().fragmented = true;

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.block_mut().fragmented = true;

        let result = trace_pointers_with_moving(
            &process,
            vec![young_parent.pointer(), mature.pointer()],
            false,
        );

        assert_eq!(mature.is_marked(), false);

        assert_eq!(result.marked, 2);
        assert_eq!(result.evacuated, 2);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_pointers_with_moving_with_mature() {
        let (_machine, _block, process) = setup();

        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent
            .get_mut()
            .add_attribute(young_child, young_child);

        young_parent.block_mut().fragmented = true;

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.block_mut().fragmented = true;

        let result = trace_pointers_with_moving(
            &process,
            vec![young_parent.pointer(), mature.pointer()],
            true,
        );

        assert_eq!(result.marked, 3);
        assert_eq!(result.evacuated, 3);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_pointers_without_moving_without_mature() {
        let (_machine, _block, process) = setup();

        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent
            .get_mut()
            .add_attribute(young_child, young_child);

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        let result = trace_pointers_without_moving(
            vec![young_parent.pointer(), mature.pointer()],
            false,
        );

        assert!(young_parent.is_marked());
        assert!(young_child.is_marked());

        assert_eq!(mature.is_marked(), false);

        assert_eq!(result.marked, 2);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_pointers_without_moving_with_mature() {
        let (_machine, _block, process) = setup();

        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent
            .get_mut()
            .add_attribute(young_child, young_child);

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        let result = trace_pointers_without_moving(
            vec![young_parent.pointer(), mature.pointer()],
            true,
        );

        assert!(young_parent.is_marked());
        assert!(young_child.is_marked());
        assert!(mature.is_marked());

        assert_eq!(result.marked, 3);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);
    }
}
