#![cfg_attr(feature="system-allocator", feature(alloc_system))]
#![feature(alloc, heap_api)]

#[cfg(feature="system-allocator")]
extern crate alloc_system;

extern crate alloc;
extern crate num_cpus;
extern crate rayon;
extern crate parking_lot;
extern crate fnv;

pub mod macros;

pub mod queue;
pub mod tagged_pointer;

pub mod binding;
pub mod block;
pub mod bytecode_parser;
pub mod compiled_code;
pub mod config;
pub mod chunk;
pub mod errors;
pub mod deref_pointer;
pub mod execution_context;
pub mod gc;
pub mod global_scope;
pub mod immix;
pub mod mailbox;
pub mod module;
pub mod module_registry;
pub mod object;
pub mod object_header;
pub mod object_pointer;
pub mod object_value;
pub mod pool;
pub mod pools;
pub mod process;
pub mod process_table;
pub mod register;
pub mod string_pool;
pub mod timer;
pub mod vm;
