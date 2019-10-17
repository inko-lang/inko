#[cfg(not(feature = "jemalloc"))]
#[global_allocator]
static A: std::alloc::System = std::alloc::System;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static A: jemallocator::Jemalloc = jemallocator::Jemalloc;

pub mod arc_without_weak;
pub mod binding;
pub mod block;
pub mod bytecode_parser;
pub mod catch_table;
pub mod chunk;
pub mod compiled_code;
pub mod config;
pub mod date_time;
pub mod deref_pointer;
pub mod directories;
pub mod duration;
pub mod error_messages;
pub mod execution_context;
pub mod ffi;
pub mod filesystem;
pub mod gc;
pub mod global_scope;
pub mod hasher;
pub mod immix;
pub mod immutable_string;
pub mod integer_operations;
pub mod macros;
pub mod mailbox;
pub mod module;
pub mod module_registry;
pub mod network_poller;
pub mod numeric;
pub mod object;
pub mod object_pointer;
pub mod object_value;
pub mod platform;
pub mod prefetch;
pub mod process;
pub mod register;
pub mod runtime_error;
pub mod runtime_panic;
pub mod scheduler;
pub mod slicing;
pub mod socket;
pub mod stacktrace;
pub mod string_pool;
pub mod tagged_pointer;
pub mod vm;
