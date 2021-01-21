//! VM functions for working with Inko strings.
use crate::execution_context::ExecutionContext;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::slicing;
use crate::vm::state::RcState;

#[inline(always)]
pub fn string_equals(
    state: &RcState,
    compare: ObjectPointer,
    compare_with: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let boolean =
        if compare.is_interned_string() && compare_with.is_interned_string() {
            if compare == compare_with {
                state.true_object
            } else {
                state.false_object
            }
        } else if compare.string_value()? == compare_with.string_value()? {
            state.true_object
        } else {
            state.false_object
        };

    Ok(boolean)
}

#[inline(always)]
pub fn string_length(
    state: &RcState,
    process: &RcProcess,
    string: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let length = process.allocate_usize(
        string.string_value()?.chars().count(),
        state.integer_prototype,
    );

    Ok(length)
}

#[inline(always)]
pub fn string_size(
    state: &RcState,
    process: &RcProcess,
    string: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let size = process
        .allocate_usize(string.string_value()?.len(), state.integer_prototype);

    Ok(size)
}

#[inline(always)]
pub fn string_concat(
    state: &RcState,
    process: &RcProcess,
    context: &ExecutionContext,
    start_reg: u16,
    amount: u16,
) -> Result<ObjectPointer, String> {
    let mut buffer = String::new();

    for register in start_reg..(start_reg + amount) {
        let ptr = context.get_register(register);

        buffer.push_str(ptr.string_value()?.as_slice());
    }

    let result = process.allocate(
        object_value::immutable_string(buffer.into()),
        state.string_prototype,
    );

    Ok(result)
}

#[inline(always)]
pub fn string_byte(
    str_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let string = str_ptr.string_value()?;
    let index = slicing::slice_index_to_usize(index_ptr, string.len())?;
    let byte = i64::from(string.as_bytes()[index]);

    Ok(ObjectPointer::integer(byte))
}
