//! Lowering of Inko MIR into LLVM IR.

pub(crate) mod builder;
pub(crate) mod constants;
pub(crate) mod context;
pub(crate) mod layouts;
pub(crate) mod method_hasher;
pub(crate) mod module;
pub(crate) mod passes;
pub(crate) mod runtime_function;
