#![allow(clippy::new_without_default)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]

// This is necessary so the zlib code is included and can be used by the
// standard library.
extern crate libz_rs_sys;

mod macros;

mod arc_without_weak;
mod config;
mod context;
mod mem;
mod memory_map;
mod network_poller;
mod notifier;
mod process;
mod rand;
mod result;
mod runtime;
mod rustls_platform_verifier;
mod scheduler;
mod socket;
mod stack;
mod state;

#[cfg(test)]
pub mod test;
