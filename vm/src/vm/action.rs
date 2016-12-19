//! Actions a VM needs to take as instructed by an instruction handler.

pub enum Action {
    /// No special action needs to be taken by the VM.
    None,

    /// The VM should jump to the instruction at the given index.
    Goto(usize),

    /// The VM should return from the current execution context.
    Return,

    /// The VM should return from an execution context and unwind the given
    /// number of frames.
    ReturnUnwind(usize),

    /// The VM should start execution of a new execution context.
    EnterContext,

    /// The VM should suspend the current process.
    Suspend,
}
