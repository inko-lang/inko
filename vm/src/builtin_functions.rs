//! A registry of builtin functions that can be called in Inko source code.
use crate::mem::Pointer;
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;
use std::io::Read;

mod array;
mod byte_array;
mod env;
mod ffi;
mod float;
mod fs;
mod hasher;
mod process;
mod random;
mod socket;
mod stdio;
mod string;
mod sys;
mod time;

/// A builtin function that can be called from Inko source code.
pub(crate) type BuiltinFunction = fn(
    &State,
    &mut Thread,
    ProcessPointer,
    &[Pointer],
) -> Result<Pointer, RuntimeError>;

/// Reads a number of bytes from a buffer into a Vec.
pub(crate) fn read_into<T: Read>(
    stream: &mut T,
    output: &mut Vec<u8>,
    size: i64,
) -> Result<i64, RuntimeError> {
    let read = if size > 0 {
        stream.take(size as u64).read_to_end(output)?
    } else {
        stream.read_to_end(output)?
    };

    Ok(read as i64)
}

/// A collection of builtin functions.
pub(crate) struct BuiltinFunctions {
    functions: Vec<BuiltinFunction>,
}

impl BuiltinFunctions {
    /// Creates a collection of builtin functions and registers all functions
    /// that Inko ships with.
    pub(crate) fn new() -> Self {
        Self {
            functions: vec![
                byte_array::byte_array_drain_to_string,
                byte_array::byte_array_to_string,
                sys::child_process_drop,
                sys::child_process_spawn,
                sys::child_process_stderr_close,
                sys::child_process_stderr_read,
                sys::child_process_stdin_close,
                sys::child_process_stdin_flush,
                sys::child_process_stdin_write_bytes,
                sys::child_process_stdin_write_string,
                sys::child_process_stdout_close,
                sys::child_process_stdout_read,
                sys::child_process_try_wait,
                sys::child_process_wait,
                env::env_arguments,
                env::env_executable,
                env::env_get,
                env::env_get_working_directory,
                env::env_home_directory,
                env::env_platform,
                env::env_set_working_directory,
                env::env_temp_directory,
                env::env_variables,
                ffi::ffi_function_attach,
                ffi::ffi_function_call,
                ffi::ffi_function_drop,
                ffi::ffi_library_drop,
                ffi::ffi_library_open,
                ffi::ffi_pointer_address,
                ffi::ffi_pointer_attach,
                ffi::ffi_pointer_from_address,
                ffi::ffi_pointer_read,
                ffi::ffi_pointer_write,
                ffi::ffi_type_alignment,
                ffi::ffi_type_size,
                fs::directory_create,
                fs::directory_create_recursive,
                fs::directory_list,
                fs::directory_remove,
                fs::directory_remove_recursive,
                fs::file_copy,
                fs::file_drop,
                fs::file_flush,
                fs::file_open_append_only,
                fs::file_open_read_append,
                fs::file_open_read_only,
                fs::file_open_read_write,
                fs::file_open_write_only,
                fs::file_read,
                fs::file_remove,
                fs::file_seek,
                fs::file_size,
                fs::file_write_bytes,
                fs::file_write_string,
                fs::path_accessed_at,
                fs::path_created_at,
                fs::path_exists,
                fs::path_is_directory,
                fs::path_is_file,
                fs::path_modified_at,
                hasher::hasher_drop,
                hasher::hasher_new,
                hasher::hasher_to_hash,
                hasher::hasher_write_int,
                process::process_stacktrace_drop,
                process::process_call_frame_line,
                process::process_call_frame_name,
                process::process_call_frame_path,
                process::process_stacktrace,
                random::random_bytes,
                random::random_float,
                random::random_float_range,
                random::random_int,
                random::random_int_range,
                socket::socket_accept_ip,
                socket::socket_accept_unix,
                socket::socket_address_pair_address,
                socket::socket_address_pair_drop,
                socket::socket_address_pair_port,
                socket::socket_allocate_ipv4,
                socket::socket_allocate_ipv6,
                socket::socket_allocate_unix,
                socket::socket_bind,
                socket::socket_connect,
                socket::socket_drop,
                socket::socket_get_broadcast,
                socket::socket_get_keepalive,
                socket::socket_get_linger,
                socket::socket_get_nodelay,
                socket::socket_get_only_v6,
                socket::socket_get_recv_size,
                socket::socket_get_reuse_address,
                socket::socket_get_reuse_port,
                socket::socket_get_send_size,
                socket::socket_get_ttl,
                socket::socket_listen,
                socket::socket_local_address,
                socket::socket_peer_address,
                socket::socket_read,
                socket::socket_receive_from,
                socket::socket_send_bytes_to,
                socket::socket_send_string_to,
                socket::socket_set_broadcast,
                socket::socket_set_keepalive,
                socket::socket_set_linger,
                socket::socket_set_nodelay,
                socket::socket_set_only_v6,
                socket::socket_set_recv_size,
                socket::socket_set_reuse_address,
                socket::socket_set_reuse_port,
                socket::socket_set_send_size,
                socket::socket_set_ttl,
                socket::socket_shutdown_read,
                socket::socket_shutdown_read_write,
                socket::socket_shutdown_write,
                socket::socket_try_clone,
                socket::socket_write_bytes,
                socket::socket_write_string,
                stdio::stderr_flush,
                stdio::stderr_write_bytes,
                stdio::stderr_write_string,
                stdio::stdin_read,
                stdio::stdout_flush,
                stdio::stdout_write_bytes,
                stdio::stdout_write_string,
                string::string_to_byte_array,
                string::string_to_float,
                string::string_to_int,
                string::string_to_lower,
                string::string_to_upper,
                time::time_monotonic,
                time::time_system,
                time::time_system_offset,
                sys::cpu_cores,
                string::string_characters,
                string::string_characters_next,
                string::string_characters_drop,
                string::string_concat_array,
                array::array_reserve,
                array::array_capacity,
                process::process_stacktrace_length,
                float::float_to_bits,
                float::float_from_bits,
                random::random_new,
                random::random_from_int,
                random::random_drop,
            ],
        }
    }

    pub(crate) fn get(&self, index: u16) -> BuiltinFunction {
        self.functions[index as usize]
    }
}
