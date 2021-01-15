//! Functions for working with non-blocking sockets.
use crate::network_poller::Interest;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::socket::Socket;
use crate::vm::state::RcState;
use std::io::Write;

macro_rules! ret {
    ($result:expr, $state:expr, $proc:expr, $sock:expr, $interest:expr) => {{
        if let Err(ref err) = $result {
            if err.should_poll() {
                $sock.register($proc, &$state.network_poller, $interest)?;
            }
        }

        $result
    }};
}

/// Allocates a new IPv4 socket.
///
/// This function requires requires one argument: the socket type.
pub fn socket_allocate_ipv4(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let kind = arguments[0].u8_value()?;
    let socket = Socket::ipv4(kind)?;
    let socket_ptr = process
        .allocate(object_value::socket(socket), state.ip_socket_prototype);

    Ok(socket_ptr)
}

/// Allocates a new IPv6 socket.
///
/// This function requires requires one argument: the socket type.
pub fn socket_allocate_ipv6(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let kind = arguments[0].u8_value()?;
    let socket = Socket::ipv6(kind)?;
    let socket_ptr = process
        .allocate(object_value::socket(socket), state.ip_socket_prototype);

    Ok(socket_ptr)
}

/// Allocates a new UNIX socket.
///
/// This function requires requires one argument: the socket type.
pub fn socket_allocate_unix(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let kind = arguments[0].u8_value()?;
    let socket = Socket::unix(kind)?;
    let socket_ptr = process
        .allocate(object_value::socket(socket), state.unix_socket_prototype);

    Ok(socket_ptr)
}

/// Writes a String to a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to write to.
/// 2. The String to write.
pub fn socket_write_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;
    let input = arguments[1].string_value()?.as_bytes();
    let res = sock
        .write(input)
        .map(|written| process.allocate_usize(written, state.integer_prototype))
        .map_err(RuntimeError::from);

    ret!(res, state, process, sock, Interest::Write)
}

/// Writes a ByteArray to a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to write to.
/// 2. The ByteArray to write.
pub fn socket_write_bytes(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;
    let input = arguments[1].byte_array_value()?;
    let res = sock
        .write(input)
        .map(|written| process.allocate_usize(written, state.integer_prototype))
        .map_err(RuntimeError::from);

    ret!(res, state, process, sock, Interest::Write)
}

/// Reads bytes from a socket into a ByteArray.
///
/// This function requires the following arguments:
///
/// 1. The socket to read from.
/// 2. The ByteArray to read into.
/// 3. The number of bytes to read.
pub fn socket_read(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;
    let buffer = arguments[1].byte_array_value_mut()?;
    let amount = arguments[2].usize_value()?;

    let result = sock
        .read(buffer, amount)
        .map(|read| process.allocate_usize(read, state.integer_prototype));

    ret!(result, state, process, sock, Interest::Read)
}

/// Listens on a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to listen on.
/// 2. The listen backlog.
pub fn socket_listen(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let backlog_ptr = arguments[1];
    let sock = arguments[0].socket_value()?;
    let backlog = backlog_ptr.i32_value()?;

    sock.listen(backlog)?;
    Ok(backlog_ptr)
}

/// Binds a socket to an address.
///
/// This function requires the following arguments:
///
/// 1. The socket to bind.
/// 2. The address to bind to.
/// 3. The port to bind to.
pub fn socket_bind(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;
    let addr = arguments[1].string_value()?;
    let port = arguments[2].u16_value()?;
    let result = sock.bind(addr, port).map(|_| state.nil_object);

    ret!(result, state, process, sock, Interest::Read)
}

/// Connects a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to connect.
/// 2. The address to connect to.
/// 3. The port to connect to.
pub fn socket_connect(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;
    let addr = arguments[1].string_value()?;
    let port = arguments[2].u16_value()?;
    let result = sock.connect(addr, port).map(|_| state.nil_object);

    ret!(result, state, process, sock, Interest::Write)
}

/// Accepts an incoming IPv4/IPv6 connection.
///
/// This function requires one argument: the socket to accept connections on.
pub fn socket_accept_ip(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;

    let result = sock.accept().map(|sock| {
        process.allocate(object_value::socket(sock), state.ip_socket_prototype)
    });

    ret!(result, state, process, sock, Interest::Read)
}

/// Accepts an incoming UNIX connection.
///
/// This function requires one argument: the socket to accept connections on.
pub fn socket_accept_unix(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;

    let result = sock.accept().map(|sock| {
        process
            .allocate(object_value::socket(sock), state.unix_socket_prototype)
    });

    ret!(result, state, process, sock, Interest::Read)
}

/// Receives data from a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to receive from.
/// 2. The ByteArray to write into.
/// 3. The number of bytes to read.
pub fn socket_receive_from(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;
    let mut buffer = arguments[1].byte_array_value_mut()?;
    let amount = arguments[2].usize_value()?;
    let result = sock
        .recv_from(&mut buffer, amount)
        .map(|(addr, port)| allocate_address_pair(state, process, addr, port));

    ret!(result, state, process, sock, Interest::Read)
}

/// Sends a ByteArray to a socket with a given address.
///
/// This function requires the following arguments:
///
/// 1. The socket to use for sending the data.
/// 2. The ByteArray to send.
/// 3. The address to send the data to.
/// 4. The port to send the data to.
pub fn socket_send_bytes_to(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;
    let buffer = arguments[1].byte_array_value()?;
    let address = arguments[2].string_value()?;
    let port = arguments[3].u16_value()?;
    let result = sock
        .send_to(buffer, address, port)
        .map(|bytes| process.allocate_usize(bytes, state.integer_prototype));

    ret!(result, state, process, sock, Interest::Write)
}

/// Sends a String to a socket with a given address.
///
/// This function requires the following arguments:
///
/// 1. The socket to use for sending the data.
/// 2. The ByteArray to send.
/// 3. The address to send the data to.
/// 4. The port to send the data to.
pub fn socket_send_string_to(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let sock = arguments[0].socket_value_mut()?;
    let buffer = arguments[1].string_value()?.as_bytes();
    let address = arguments[2].string_value()?;
    let port = arguments[3].u16_value()?;
    let result = sock
        .send_to(buffer, address, port)
        .map(|bytes| process.allocate_usize(bytes, state.integer_prototype));

    ret!(result, state, process, sock, Interest::Write)
}

/// Shuts down a socket for reading.
///
/// This function requires one argument: the socket to shut down.
pub fn socket_shutdown_read(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0]
        .socket_value()?
        .shutdown_read()
        .map(|_| state.nil_object)
}

/// Shuts down a socket for writing.
///
/// This function requires one argument: the socket to shut down.
pub fn socket_shutdown_write(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0]
        .socket_value()?
        .shutdown_write()
        .map(|_| state.nil_object)
}

/// Shuts down a socket for reading and writing.
///
/// This function requires one argument: the socket to shut down.
pub fn socket_shutdown_read_write(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0]
        .socket_value()?
        .shutdown_read_write()
        .map(|_| state.nil_object)
}

/// Returns the local address of a socket.
///
/// This function requires one argument: the socket to return the address for.
pub fn socket_local_address(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0]
        .socket_value()?
        .local_address()
        .map(|(addr, port)| allocate_address_pair(state, process, addr, port))
}

/// Returns the peer address of a socket.
///
/// This function requires one argument: the socket to return the address for.
pub fn socket_peer_address(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0]
        .socket_value()?
        .peer_address()
        .map(|(addr, port)| allocate_address_pair(state, process, addr, port))
}

/// Returns the value of the `IP_TTL` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_ttl(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate_usize(
        arguments[0].socket_value()?.ttl()?,
        state.integer_prototype,
    ))
}

/// Returns the value of the `IPV6_ONLY` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_only_v6(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    if arguments[0].socket_value()?.only_v6()? {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Returns the value of the `TCP_NODELAY` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_nodelay(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    if arguments[0].socket_value()?.nodelay()? {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Returns the value of the `SO_BROADCAST` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_broadcast(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    if arguments[0].socket_value()?.broadcast()? {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Returns the value of the `SO_LINGER` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_linger(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate(
        object_value::float(arguments[0].socket_value()?.linger()?),
        state.float_prototype,
    ))
}

/// Returns the value of the `SO_RCVBUF` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_recv_size(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate_usize(
        arguments[0].socket_value()?.recv_buffer_size()?,
        state.integer_prototype,
    ))
}

/// Returns the value of the `SO_SNDBUF` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_send_size(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate_usize(
        arguments[0].socket_value()?.send_buffer_size()?,
        state.integer_prototype,
    ))
}

/// Returns the value of the `SO_KEEPALIVE` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_keepalive(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate(
        object_value::float(arguments[0].socket_value()?.keepalive()?),
        state.float_prototype,
    ))
}

/// Returns the value of the `IP_MULTICAST_LOOP` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_loop_v4(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    if arguments[0].socket_value()?.multicast_loop_v4()? {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Returns the value of the `IPV6_MULTICAST_LOOP` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_loop_v6(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    if arguments[0].socket_value()?.multicast_loop_v6()? {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Returns the value of the `IP_MULTICAST_TTL` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_ttl_v4(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate_usize(
        arguments[0].socket_value()?.multicast_ttl_v4()?,
        state.integer_prototype,
    ))
}

/// Returns the value of the `IPV6_MULTICAST_HOPS` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_hops_v6(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate_usize(
        arguments[0].socket_value()?.multicast_hops_v6()?,
        state.integer_prototype,
    ))
}

/// Returns the value of the `IP_MULTICAST_IF` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_if_v4(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate(
        object_value::string(arguments[0].socket_value()?.multicast_if_v4()?),
        state.string_prototype,
    ))
}

/// Returns the value of the `IPV6_MULTICAST_IF` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_if_v6(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate_usize(
        arguments[0].socket_value()?.multicast_if_v6()?,
        state.integer_prototype,
    ))
}

/// Returns the value of the `IPV6_UNICAST_HOPS` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_unicast_hops_v6(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate_usize(
        arguments[0].socket_value()?.unicast_hops_v6()?,
        state.integer_prototype,
    ))
}

/// Returns the value of the `SO_REUSEADDR` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_reuse_address(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    if arguments[0].socket_value()?.reuse_address()? {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Returns the value of the `SO_REUSEPORT` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_reuse_port(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    if arguments[0].socket_value()?.reuse_port()? {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Sets the value of the `IP_TTL` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_ttl(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_ttl(value.u32_value()?)?;
    Ok(value)
}

/// Sets the value of the `IPV6_ONLY` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_only_v6(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_only_v6(is_true!(state, arguments[1]))?;
    Ok(value)
}

/// Sets the value of the `TCP_NODELAY` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_nodelay(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_nodelay(is_true!(state, arguments[1]))?;
    Ok(value)
}

/// Sets the value of the `SO_BROADCAST` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_broadcast(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_broadcast(is_true!(state, arguments[1]))?;
    Ok(value)
}

/// Sets the value of the `SO_LINGER` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_linger(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_linger(value.float_value()?)?;
    Ok(value)
}

/// Sets the value of the `SO_RCVBUF` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_recv_size(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_recv_buffer_size(value.usize_value()?)?;
    Ok(value)
}

/// Sets the value of the `SO_SNDBUF` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_send_size(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_send_buffer_size(value.usize_value()?)?;
    Ok(value)
}

/// Sets the value of the `SO_KEEPALIVE` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_keepalive(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_keepalive(value.float_value()?)?;
    Ok(value)
}

/// Sets the value of the `IP_MULTICAST_LOOP` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_loop_v4(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_multicast_loop_v4(is_true!(state, arguments[1]))?;
    Ok(value)
}

/// Sets the value of the `IPV6_MULTICAST_LOOP` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_loop_v6(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_multicast_loop_v6(is_true!(state, arguments[1]))?;
    Ok(value)
}

/// Sets the value of the `IP_MULTICAST_TTL` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_ttl_v4(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_multicast_ttl_v4(value.u32_value()?)?;
    Ok(value)
}

/// Sets the value of the `IPV6_MULTICAST_HOPS` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_hops_v6(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_multicast_hops_v6(value.u32_value()?)?;
    Ok(value)
}

/// Sets the value of the `IP_MULTICAST_IF` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_if_v4(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_multicast_if_v4(value.string_value()?)?;
    Ok(value)
}

/// Sets the value of the `IPV6_MULTICAST_IF` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_if_v6(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_multicast_if_v6(value.u32_value()?)?;
    Ok(value)
}

/// Sets the value of the `IPV6_UNICAST_HOPS` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_unicast_hops_v6(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_unicast_hops_v6(value.u32_value()?)?;
    Ok(value)
}

/// Sets the value of the `SO_REUSEADDR` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_reuse_address(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_reuse_address(is_true!(state, arguments[1]))?;
    Ok(value)
}

/// Sets the value of the `SO_REUSEPORT` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_reuse_port(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[1];

    arguments[0]
        .socket_value_mut()?
        .set_reuse_port(is_true!(state, arguments[1]))?;
    Ok(value)
}

fn allocate_address_pair(
    state: &RcState,
    process: &RcProcess,
    addr: String,
    port: i64,
) -> ObjectPointer {
    let addr_ptr =
        process.allocate(object_value::string(addr), state.string_prototype);

    let port_ptr = ObjectPointer::integer(port);

    process.allocate(
        object_value::array(vec![addr_ptr, port_ptr]),
        state.array_prototype,
    )
}

register!(
    socket_allocate_ipv4,
    socket_allocate_ipv6,
    socket_allocate_unix,
    socket_write_string,
    socket_write_bytes,
    socket_read,
    socket_listen,
    socket_bind,
    socket_connect,
    socket_accept_ip,
    socket_accept_unix,
    socket_receive_from,
    socket_send_bytes_to,
    socket_send_string_to,
    socket_shutdown_read,
    socket_shutdown_write,
    socket_shutdown_read_write,
    socket_local_address,
    socket_peer_address,
    socket_get_ttl,
    socket_get_only_v6,
    socket_get_nodelay,
    socket_get_broadcast,
    socket_get_linger,
    socket_get_recv_size,
    socket_get_send_size,
    socket_get_keepalive,
    socket_get_multicast_loop_v4,
    socket_get_multicast_loop_v6,
    socket_get_multicast_ttl_v4,
    socket_get_multicast_hops_v6,
    socket_get_multicast_if_v4,
    socket_get_multicast_if_v6,
    socket_get_unicast_hops_v6,
    socket_get_reuse_address,
    socket_get_reuse_port,
    socket_set_ttl,
    socket_set_only_v6,
    socket_set_nodelay,
    socket_set_broadcast,
    socket_set_linger,
    socket_set_recv_size,
    socket_set_send_size,
    socket_set_keepalive,
    socket_set_multicast_loop_v4,
    socket_set_multicast_loop_v6,
    socket_set_multicast_ttl_v4,
    socket_set_multicast_hops_v6,
    socket_set_multicast_if_v4,
    socket_set_multicast_if_v6,
    socket_set_unicast_hops_v6,
    socket_set_reuse_address,
    socket_set_reuse_port
);
