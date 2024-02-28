#![allow(clippy::new_without_default)]
#![allow(clippy::enum_variant_names)]

mod diagnostics;
mod hir;
mod incremental;
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
