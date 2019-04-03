//! VM functions for working with Inko integers.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;
use num_traits::ToPrimitive;

pub fn to_float(
    state: &RcState,
    process: &RcProcess,
    integer: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = if integer.is_bigint() {
        let bigint = integer.bigint_value().unwrap();

        if let Some(float) = bigint.to_f64() {
            float
        } else {
            return Err(format!("Failed to convert {} to a float", bigint));
        }
    } else {
        integer.integer_value()? as f64
    };

    Ok(process.allocate(object_value::float(result), state.float_prototype))
}

pub fn to_string(
    state: &RcState,
    process: &RcProcess,
    integer: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = if integer.is_integer() {
        integer.integer_value()?.to_string()
    } else if integer.is_bigint() {
        integer.bigint_value()?.to_string()
    } else {
        return Err(
            "Only integers are supported for this operation".to_string()
        );
    };

    Ok(process.allocate(object_value::string(result), state.string_prototype))
}
