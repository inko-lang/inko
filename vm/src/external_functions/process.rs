//! Functions for Inko processes.
use crate::execution_context::ExecutionContext;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Returns a stacktrace for the current process.
///
/// This function requires two arguments:
///
/// 1. The number of stack frames to include.
/// 2. The number of stack frames to skip, starting at the current frame.
pub fn process_stacktrace(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let limit = arguments[0].usize_value()?;
    let skip = arguments[1].usize_value()?;

    let mut trace = if limit > 0 {
        Vec::with_capacity(limit)
    } else {
        Vec::new()
    };

    let mut contexts: Vec<&ExecutionContext> = {
        let iter = process.contexts().into_iter().skip(skip);

        if limit > 0 {
            iter.take(limit).collect()
        } else {
            iter.collect()
        }
    };

    contexts.reverse();

    for context in contexts {
        let file = context.code.file;
        let name = context.code.name;
        let line = ObjectPointer::integer(i64::from(context.line()));
        let tuple = process.allocate(
            object_value::array(vec![file, name, line]),
            state.array_prototype,
        );

        trace.push(tuple);
    }

    Ok(process.allocate(object_value::array(trace), state.array_prototype))
}

register!(process_stacktrace);
