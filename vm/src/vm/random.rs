//! VM functions for generating random values.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::state::RcState;
use num_bigint::{BigInt, ToBigInt};

const INTEGER: i64 = 0;
const INCREMENTAL_INTEGER: i64 = 1;
const FLOAT: i64 = 2;

macro_rules! verify_min_max {
    ($min:expr, $max:expr) => {
        if $min >= $max {
            return Err(format!(
                "The minimum range value {} must be smaller than the maximum {}",
                $min, $max
            ));
        }
    };
}

pub fn number(
    state: &RcState,
    process: &RcProcess,
    worker: &mut ProcessWorker,
    kind_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let kind = kind_ptr.integer_value()?;

    match kind {
        INTEGER => Ok(process
            .allocate_i64(worker.random_number(), state.integer_prototype)),
        INCREMENTAL_INTEGER => Ok(process
            .allocate_u64(worker.random_number(), state.integer_prototype)),
        FLOAT => Ok(process.allocate(
            object_value::float(worker.random_number()),
            state.float_prototype,
        )),
        _ => Err(format!(
            "{} is not a valid type to generate a random value for",
            kind
        )),
    }
}

pub fn range(
    state: &RcState,
    process: &RcProcess,
    worker: &mut ProcessWorker,
    min_ptr: ObjectPointer,
    max_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    if min_ptr.is_integer() && max_ptr.is_integer() {
        let min = min_ptr.integer_value()?;
        let max = max_ptr.integer_value()?;

        verify_min_max!(min, max);

        Ok(process.allocate_i64(
            worker.random_number_between(min, max),
            state.integer_prototype,
        ))
    } else if min_ptr.is_bigint() || max_ptr.is_bigint() {
        let min = to_bigint(min_ptr)?;
        let max = to_bigint(max_ptr)?;

        verify_min_max!(min, max);

        Ok(process.allocate(
            object_value::bigint(worker.random_bigint_between(&min, &max)),
            state.integer_prototype,
        ))
    } else if min_ptr.is_float() || max_ptr.is_float() {
        let min = min_ptr.float_value()?;
        let max = max_ptr.float_value()?;

        verify_min_max!(min, max);

        Ok(process.allocate(
            object_value::float(worker.random_number_between(min, max)),
            state.float_prototype,
        ))
    } else {
        Err(
            "Random values can only be generated for Integers and Floats"
                .to_string(),
        )
    }
}

pub fn bytes(
    state: &RcState,
    process: &RcProcess,
    worker: &mut ProcessWorker,
    size_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let size = size_ptr.usize_value()?;
    let bytes = worker.random_bytes(size)?;

    Ok(process
        .allocate(object_value::byte_array(bytes), state.byte_array_prototype))
}

fn to_bigint(pointer: ObjectPointer) -> Result<BigInt, String> {
    if let Ok(bigint) = pointer.bigint_value() {
        Ok(bigint.clone())
    } else {
        Ok(pointer.integer_value()?.to_bigint().unwrap())
    }
}
