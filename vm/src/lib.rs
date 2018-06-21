#![cfg_attr(feature = "prefetch", feature(core_intrinsics))]
#![feature(allocator_api, alloc)]

extern crate alloc;
extern crate colored;
extern crate fnv;
extern crate num_cpus;
extern crate num_integer;
extern crate parking_lot;
extern crate rayon;
extern crate rug;
extern crate time;

pub mod macros;

pub mod arc_without_weak;
pub mod binding;
pub mod block;
pub mod byte_array;
pub mod bytecode_parser;
pub mod catch_table;
pub mod chunk;
pub mod compiled_code;
pub mod config;
pub mod date_time;
pub mod deref_pointer;
pub mod error_messages;
pub mod execution_context;
pub mod filesystem;
pub mod gc;
pub mod global_scope;
pub mod hasher;
pub mod immix;
pub mod integer_operations;
pub mod io;
pub mod mailbox;
pub mod module;
pub mod module_registry;
pub mod numeric;
pub mod object;
pub mod object_pointer;
pub mod object_value;
pub mod pool;
pub mod pools;
pub mod process;
pub mod process_table;
pub mod queue;
pub mod register;
pub mod runtime_panic;
pub mod slicing;
pub mod stacktrace;
pub mod string_pool;
pub mod suspension_list;
pub mod tagged_pointer;
pub mod timer;
pub mod vm;
