#![cfg_attr(feature = "cargo-clippy", allow(clippy::new_without_default))]
#![cfg_attr(feature = "cargo-clippy", allow(clippy::enum_variant_names))]

mod diagnostics;
mod hir;
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
