#![cfg_attr(feature = "prefetch", feature(core_intrinsics))]
#![cfg_attr(feature = "system-allocator",
            feature(alloc_system, global_allocator))]
#![feature(allocator_api, alloc)]

#[cfg(feature = "system-allocator")]
extern crate alloc_system;

#[cfg(feature = "system-allocator")]
use alloc_system::System;

#[cfg(feature = "system-allocator")]
#[global_allocator]
static A: System = System;

extern crate alloc;
extern crate colored;
extern crate fnv;
extern crate num_bigint;
extern crate num_cpus;
extern crate parking_lot;
extern crate rayon;

pub mod macros;

pub mod arc_without_weak;
pub mod binding;
pub mod block;
pub mod bytecode_parser;
pub mod catch_table;
pub mod chunk;
pub mod compiled_code;
pub mod config;
pub mod deref_pointer;
pub mod error_messages;
pub mod execution_context;
pub mod gc;
pub mod global_scope;
pub mod integer_operations;
pub mod immix;
pub mod mailbox;
pub mod module;
pub mod module_registry;
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
pub mod string_pool;
pub mod suspension_list;
pub mod tagged_pointer;
pub mod timer;
pub mod vm;
