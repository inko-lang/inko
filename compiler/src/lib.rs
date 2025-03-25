#![allow(clippy::new_without_default)]
#![allow(clippy::enum_variant_names)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::needless_range_loop)]

pub mod diagnostics;
pub mod docs;
pub mod format;
mod hir;
mod incremental;
mod json;
mod linker;
mod llvm;
mod mir;
mod modules_parser;
pub mod pkg;
mod presenters;
mod state;
mod symbol_names;
pub mod target;
mod type_check;

#[cfg(test)]
mod test;

pub mod compiler;
pub mod config;
