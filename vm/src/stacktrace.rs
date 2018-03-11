//! Runtime stack traces.
use execution_context::ExecutionContext;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;
use vm::state::RcState;

/// Produces a stacktrace containing up to N stack frames.
pub fn allocate_stacktrace(
    process: &RcProcess,
    state: &RcState,
    limit: Option<usize>,
    skip: usize,
) -> ObjectPointer {
    let mut trace = if let Some(limit) = limit {
        Vec::with_capacity(limit)
    } else {
        Vec::new()
    };

    let mut contexts: Vec<&ExecutionContext> = {
        let iter = process.context().contexts().skip(skip);

        if let Some(limit) = limit {
            iter.take(limit).collect()
        } else {
            iter.collect()
        }
    };

    contexts.reverse();

    for context in contexts {
        let file = process.allocate(
            object_value::string(context.code.file.clone()),
            state.string_prototype,
        );

        let name = process.allocate(
            object_value::string(context.code.name.clone()),
            state.string_prototype,
        );

        let line = ObjectPointer::integer(context.line as i64);

        let tuple = process.allocate(
            object_value::array(vec![file, name, line]),
            state.array_prototype,
        );

        trace.push(tuple);
    }

    process.allocate(object_value::array(trace), state.array_prototype)
}
