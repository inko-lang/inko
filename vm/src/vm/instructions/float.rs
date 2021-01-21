//! VM functions for working with Inko floats.
use crate::object_pointer::ObjectPointer;
use crate::vm::state::RcState;
use float_cmp::ApproxEqUlps;

#[inline(always)]
pub fn float_equals(
    state: &RcState,
    compare_ptr: ObjectPointer,
    compare_with_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let compare = compare_ptr.float_value()?;
    let compare_with = compare_with_ptr.float_value()?;

    let boolean = if !compare.is_nan()
        && !compare_with.is_nan()
        && compare.approx_eq_ulps(&compare_with, 2)
    {
        state.true_object
    } else {
        state.false_object
    };

    Ok(boolean)
}
