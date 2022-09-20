//! VM functions for handling builtin functions.
use crate::mem::Pointer;
use crate::process::{ProcessPointer, TaskPointer};
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;

#[inline(always)]
pub(crate) fn call(
    state: &State,
    thread: &mut Thread,
    process: ProcessPointer,
    task: TaskPointer,
    func_index: u16,
) -> Result<Pointer, RuntimeError> {
    let func = state.builtin_functions.get(func_index);

    // We keep the arguments as-is, as the function may suspend the current
    // process and require retrying the current instruction.
    let args = &task.stack;

    func(state, thread, process, args)
}
