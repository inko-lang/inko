//! VM functions for working with Inko objects.
use immix::copy_object::CopyObject;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;
use vm::state::RcState;

/// Creates a new object.
pub fn create(
    state: &RcState,
    process: &RcProcess,
    perm_ptr: ObjectPointer,
    proto_ptr: Option<ObjectPointer>,
) -> ObjectPointer {
    let is_permanent = perm_ptr != state.false_object;

    let obj = if is_permanent {
        state.permanent_allocator.lock().allocate_empty()
    } else {
        process.allocate_empty()
    };

    if let Some(proto) = proto_ptr {
        let proto_to_use = if is_permanent && !proto.is_permanent() {
            state.permanent_allocator.lock().copy_object(proto)
        } else {
            proto
        };

        obj.get_mut().set_prototype(proto_to_use);
    }

    obj
}

/// Returns a prototype for the given numeric ID.
///
/// This method operates on an i64 instead of some sort of enum, as enums
/// can not be represented in Inko code.
pub fn prototype_for_identifier(
    state: &RcState,
    id: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let id_int = id.integer_value()?;

    let proto = match id_int {
        0 => state.object_prototype,
        1 => state.integer_prototype,
        2 => state.float_prototype,
        3 => state.string_prototype,
        4 => state.array_prototype,
        5 => state.block_prototype,
        6 => state.boolean_prototype,
        7 => state.read_only_file_prototype,
        8 => state.write_only_file_prototype,
        9 => state.read_write_file_prototype,
        10 => state.byte_array_prototype,
        11 => state.hasher_prototype,
        12 => state.library_prototype,
        13 => state.function_prototype,
        14 => state.pointer_prototype,
        15 => state.process_prototype,
        _ => return Err(format!("Invalid prototype identifier: {}", id_int)),
    };

    Ok(proto)
}

pub fn get_attribute(
    state: &RcState,
    rec_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
) -> ObjectPointer {
    let name = state.intern_pointer(name_ptr).unwrap_or_else(|_| name_ptr);

    rec_ptr
        .lookup_attribute(&state, name)
        .unwrap_or_else(|| state.nil_object)
}

pub fn set_attribute(
    state: &RcState,
    process: &RcProcess,
    target_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
    value_ptr: ObjectPointer,
) -> ObjectPointer {
    if target_ptr.is_immutable() {
        return state.nil_object;
    }

    let name = state.intern_pointer(name_ptr).unwrap_or_else(|_| {
        copy_if_permanent!(state.permanent_allocator, name_ptr, target_ptr)
    });

    let value =
        copy_if_permanent!(state.permanent_allocator, value_ptr, target_ptr);

    target_ptr.add_attribute(&process, name, value);

    value
}

pub fn set_attribute_to_object(
    state: &RcState,
    process: &RcProcess,
    obj_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
) -> ObjectPointer {
    if obj_ptr.is_immutable() {
        return state.nil_object;
    }

    let name = state.intern_pointer(name_ptr).unwrap_or_else(|_| {
        copy_if_permanent!(state.permanent_allocator, name_ptr, obj_ptr)
    });

    if let Some(ptr) = obj_ptr.get().lookup_attribute_in_self(name) {
        ptr
    } else {
        let value = object_value::none();
        let proto = state.object_prototype;

        let ptr = if obj_ptr.is_permanent() {
            state
                .permanent_allocator
                .lock()
                .allocate_with_prototype(value, proto)
        } else {
            process.allocate(value, proto)
        };

        obj_ptr.add_attribute(&process, name, ptr);

        ptr
    }
}

pub fn set_prototype(
    state: &RcState,
    process: &RcProcess,
    src_ptr: ObjectPointer,
    proto_ptr: ObjectPointer,
) -> ObjectPointer {
    if src_ptr.is_immutable() {
        return state.nil_object;
    }

    let prototype =
        copy_if_permanent!(state.permanent_allocator, proto_ptr, src_ptr);

    src_ptr.set_prototype(prototype);

    process.write_barrier(src_ptr, prototype);

    prototype
}

pub fn get_prototype(state: &RcState, src_ptr: ObjectPointer) -> ObjectPointer {
    src_ptr
        .prototype(&state)
        .unwrap_or_else(|| state.nil_object)
}

pub fn equal(
    state: &RcState,
    compare: ObjectPointer,
    compare_with: ObjectPointer,
) -> ObjectPointer {
    if compare == compare_with {
        state.true_object
    } else {
        state.false_object
    }
}

pub fn kind_of(
    state: &RcState,
    compare: ObjectPointer,
    compare_with: ObjectPointer,
) -> ObjectPointer {
    if compare.is_kind_of(&state, compare_with) {
        state.true_object
    } else {
        state.false_object
    }
}

pub fn attribute_exists(
    state: &RcState,
    source_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
) -> ObjectPointer {
    let name = state.intern_pointer(name_ptr).unwrap_or_else(|_| name_ptr);

    if source_ptr.lookup_attribute(&state, name).is_some() {
        state.true_object
    } else {
        state.false_object
    }
}

pub fn remove_attribute(
    state: &RcState,
    rec_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
) -> ObjectPointer {
    if rec_ptr.is_immutable() {
        return state.nil_object;
    }

    let name = state.intern_pointer(name_ptr).unwrap_or_else(|_| name_ptr);

    if let Some(attribute) = rec_ptr.get_mut().remove_attribute(name) {
        attribute
    } else {
        state.nil_object
    }
}

pub fn attribute_names(
    state: &RcState,
    process: &RcProcess,
    rec_ptr: ObjectPointer,
) -> ObjectPointer {
    let attributes = rec_ptr.attribute_names();

    process.allocate(object_value::array(attributes), state.array_prototype)
}

pub fn copy_blocks(
    state: &RcState,
    target_ptr: ObjectPointer,
    source_ptr: ObjectPointer,
) {
    if target_ptr.is_immutable() || source_ptr.is_immutable() {
        return;
    }

    let object = target_ptr.get_mut();
    let to_impl = source_ptr.get();

    if let Some(map) = to_impl.attributes_map() {
        for (key, val) in map.iter() {
            if val.block_value().is_err() {
                continue;
            }

            let block =
                copy_if_permanent!(state.permanent_allocator, *val, target_ptr);

            object.add_attribute(*key, block);
        }
    }
}

pub fn drop_value(pointer: ObjectPointer) {
    let object = pointer.get_mut();

    if object.value.is_some() {
        drop(object.value.take());

        if !object.has_attributes() {
            pointer.unmark_for_finalization();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;
    use vm::state::State;

    #[test]
    fn test_prototype_for_identifier() {
        let state = State::with_rc(Config::new(), &[]);

        assert!(
            prototype_for_identifier(&state, ObjectPointer::integer(2))
                .unwrap()
                == state.float_prototype
        );

        assert!(
            prototype_for_identifier(&state, ObjectPointer::integer(5))
                .unwrap()
                == state.block_prototype
        );

        assert!(prototype_for_identifier(&state, ObjectPointer::integer(-1))
            .is_err());
    }
}
