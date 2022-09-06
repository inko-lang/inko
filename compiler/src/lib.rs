#![cfg_attr(feature = "cargo-clippy", allow(clippy::new_without_default))]
#![cfg_attr(feature = "cargo-clippy", allow(clippy::enum_variant_names))]

mod codegen;
mod diagnostics;
mod hir;
mod mir;
mod modules_parser;
mod presenters;
mod source_paths;
mod state;
mod type_check;

#[cfg(test)]
mod test;

pub mod compiler;
pub mod config;
