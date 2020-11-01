//! VM functions for working with Inko objects.
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

#[inline(always)]
pub fn allocate(
    process: &RcProcess,
    proto_ptr: ObjectPointer,
) -> ObjectPointer {
    process.allocate(object_value::none(), proto_ptr)
}

#[inline(always)]
pub fn allocate_permanent(
    state: &RcState,
    proto_ptr: ObjectPointer,
) -> ObjectPointer {
    let proto_to_use = if proto_ptr.is_permanent() {
        proto_ptr
    } else {
        state.permanent_allocator.lock().copy_object(proto_ptr)
    };

    state
        .permanent_allocator
        .lock()
        .allocate_with_prototype(object_value::none(), proto_to_use)
}

/// Returns a prototype for the given numeric ID.
///
/// This method operates on an i64 instead of some sort of enum, as enums
/// can not be represented in Inko code.
#[inline(always)]
pub fn get_builtin_prototype(
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
        7 => state.byte_array_prototype,
        8 => state.nil_prototype,
        9 => state.module_prototype,
        10 => state.ffi_library_prototype,
        11 => state.ffi_function_prototype,
        12 => state.ffi_pointer_prototype,
        13 => state.ip_socket_prototype,
        14 => state.unix_socket_prototype,
        15 => state.process_prototype,
        16 => state.read_only_file_prototype,
        17 => state.write_only_file_prototype,
        18 => state.read_write_file_prototype,
        _ => return Err(format!("Invalid prototype identifier: {}", id_int)),
    };

    Ok(proto)
}

#[inline(always)]
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

#[inline(always)]
pub fn get_attribute_in_self(
    state: &RcState,
    rec_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
) -> ObjectPointer {
    let name = state.intern_pointer(name_ptr).unwrap_or_else(|_| name_ptr);

    rec_ptr
        .lookup_attribute_in_self(&state, name)
        .unwrap_or_else(|| state.nil_object)
}

#[inline(always)]
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

#[inline(always)]
pub fn get_prototype(state: &RcState, src_ptr: ObjectPointer) -> ObjectPointer {
    src_ptr
        .prototype(&state)
        .unwrap_or_else(|| state.nil_object)
}

#[inline(always)]
pub fn object_equals(
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

#[inline(always)]
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

#[inline(always)]
pub fn get_attribute_names(
    state: &RcState,
    process: &RcProcess,
    rec_ptr: ObjectPointer,
) -> ObjectPointer {
    let attributes = rec_ptr.attribute_names();

    process.allocate(object_value::array(attributes), state.array_prototype)
}

#[inline(always)]
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

#[inline(always)]
pub fn close(pointer: ObjectPointer) {
    pointer.get_mut().value.close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::vm::state::State;

    #[test]
    fn test_get_builtin_prototype() {
        let state = State::with_rc(Config::new(), &[]);

        assert!(
            get_builtin_prototype(&state, ObjectPointer::integer(2)).unwrap()
                == state.float_prototype
        );

        assert!(
            get_builtin_prototype(&state, ObjectPointer::integer(5)).unwrap()
                == state.block_prototype
        );

        assert!(
            get_builtin_prototype(&state, ObjectPointer::integer(-1)).is_err()
        );
    }
}
