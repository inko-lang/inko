use crate::network_poller::Interest;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::socket::Socket;
use crate::vm::instructions::io;
use crate::vm::state::RcState;

const TTL: i64 = 0;
const ONLY_V6: i64 = 1;
const NODELAY: i64 = 2;
const BROADCAST: i64 = 3;
const LINGER: i64 = 4;
const RECV_SIZE: i64 = 5;
const SEND_SIZE: i64 = 6;
const KEEPALIVE: i64 = 7;
const MULTICAST_LOOP_V4: i64 = 8;
const MULTICAST_LOOP_V6: i64 = 9;
const MULTICAST_TTL_V4: i64 = 10;
const MULTICAST_HOPS_V6: i64 = 11;
const MULTICAST_IF_V4: i64 = 12;
const MULTICAST_IF_V6: i64 = 13;
const UNICAST_HOPS_V6: i64 = 14;
const REUSE_ADDRESS: i64 = 15;
const REUSE_PORT: i64 = 16;

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

#[inline(always)]
pub fn socket_create(
    state: &RcState,
    process: &RcProcess,
    domain_ptr: ObjectPointer,
    kind_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let domain = domain_ptr.u8_value()?;
    let kind = kind_ptr.u8_value()?;
    let socket = Socket::new(domain, kind)?;
    let proto_ptr = socket_prototype(state, &socket);

    let socket_ptr = process.allocate(object_value::socket(socket), proto_ptr);

    Ok(socket_ptr)
}

#[inline(always)]
pub fn socket_write(
    state: &RcState,
    process: &RcProcess,
    socket_ptr: ObjectPointer,
    input_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let sock = socket_ptr.socket_value_mut()?;
    let res = io::io_write(state, process, sock, input_ptr);

    ret!(res, state, process, sock, Interest::Write)
}

#[inline(always)]
pub fn socket_read(
    state: &RcState,
    process: &RcProcess,
    socket_ptr: ObjectPointer,
    buff_ptr: ObjectPointer,
    amount_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let sock = socket_ptr.socket_value_mut()?;
    let buffer = buff_ptr.byte_array_value_mut()?;
    let amount = amount_ptr.usize_value()?;

    let result = sock
        .read(buffer, amount)
        .map(|read| process.allocate_usize(read, state.integer_prototype));

    ret!(result, state, process, sock, Interest::Read)
}

#[inline(always)]
pub fn socket_listen(
    socket_ptr: ObjectPointer,
    backlog_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let sock = socket_ptr.socket_value()?;
    let backlog = backlog_ptr.i32_value()?;

    sock.listen(backlog)?;

    Ok(backlog_ptr)
}

#[inline(always)]
pub fn socket_bind(
    state: &RcState,
    process: &RcProcess,
    socket_ptr: ObjectPointer,
    addr_ptr: ObjectPointer,
    port_ptr: ObjectPointer,
) -> Result<(), RuntimeError> {
    let sock = socket_ptr.socket_value_mut()?;
    let addr = addr_ptr.string_value()?;
    let port = port_ptr.u16_value()?;
    let result = sock.bind(addr, port);

    ret!(result, state, process, sock, Interest::Read)
}

#[inline(always)]
pub fn socket_connect(
    state: &RcState,
    process: &RcProcess,
    socket_ptr: ObjectPointer,
    addr_ptr: ObjectPointer,
    port_ptr: ObjectPointer,
) -> Result<(), RuntimeError> {
    let sock = socket_ptr.socket_value_mut()?;
    let addr = addr_ptr.string_value()?;
    let port = port_ptr.u16_value()?;
    let result = sock.connect(addr, port);

    ret!(result, state, process, sock, Interest::Write)
}

#[inline(always)]
pub fn socket_accept(
    state: &RcState,
    process: &RcProcess,
    socket_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let sock = socket_ptr.socket_value_mut()?;
    let proto_ptr = socket_prototype(state, &sock);

    let result = sock
        .accept()
        .map(|sock| process.allocate(object_value::socket(sock), proto_ptr));

    ret!(result, state, process, sock, Interest::Read)
}

#[inline(always)]
pub fn socket_receive_from(
    state: &RcState,
    process: &RcProcess,
    socket_ptr: ObjectPointer,
    buffer_ptr: ObjectPointer,
    amount_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let sock = socket_ptr.socket_value_mut()?;
    let mut buffer = buffer_ptr.byte_array_value_mut()?;
    let amount = amount_ptr.usize_value()?;
    let result = sock
        .recv_from(&mut buffer, amount)
        .map(|(addr, port)| allocate_address_pair(state, process, addr, port));

    ret!(result, state, process, sock, Interest::Read)
}

#[inline(always)]
pub fn socket_send_to(
    state: &RcState,
    process: &RcProcess,
    socket_pointer: ObjectPointer,
    buffer_pointer: ObjectPointer,
    address_pointer: ObjectPointer,
    port_pointer: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let buffer = io::buffer_to_write(&buffer_pointer)?;
    let sock = socket_pointer.socket_value_mut()?;
    let address = address_pointer.string_value()?;
    let port = port_pointer.u16_value()?;
    let result = sock
        .send_to(buffer, address, port)
        .map(|bytes| process.allocate_usize(bytes, state.integer_prototype));

    ret!(result, state, process, sock, Interest::Write)
}

#[inline(always)]
pub fn socket_address(
    state: &RcState,
    process: &RcProcess,
    socket_pointer: ObjectPointer,
    kind_pointer: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let sock = socket_pointer.socket_value()?;
    let kind = kind_pointer.integer_value()?;

    match kind {
        0 => sock.local_address(),
        1 => sock.peer_address(),
        _ => Err(RuntimeError::Panic(format!(
            "{} is not a valid type of socket address",
            kind
        ))),
    }
    .map(|(addr, port)| allocate_address_pair(state, process, addr, port))
}

#[inline(always)]
pub fn socket_set_option(
    state: &RcState,
    socket_pointer: ObjectPointer,
    option_pointer: ObjectPointer,
    val_pointer: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let sock = socket_pointer.socket_value()?;
    let option = option_pointer.integer_value()?;

    match option {
        TTL => sock.set_ttl(val_pointer.u32_value()?)?,
        ONLY_V6 => sock.set_only_v6(is_true!(state, val_pointer))?,
        NODELAY => sock.set_nodelay(is_true!(state, val_pointer))?,
        BROADCAST => sock.set_broadcast(is_true!(state, val_pointer))?,
        LINGER => sock.set_linger(val_pointer.float_value()?)?,
        RECV_SIZE => sock.set_recv_buffer_size(val_pointer.usize_value()?)?,
        SEND_SIZE => sock.set_send_buffer_size(val_pointer.usize_value()?)?,
        KEEPALIVE => sock.set_keepalive(val_pointer.float_value()?)?,
        MULTICAST_LOOP_V4 => {
            sock.set_multicast_loop_v4(is_true!(state, val_pointer))?
        }
        MULTICAST_LOOP_V6 => {
            sock.set_multicast_loop_v6(is_true!(state, val_pointer))?
        }
        MULTICAST_TTL_V4 => {
            sock.set_multicast_ttl_v4(val_pointer.u32_value()?)?
        }
        MULTICAST_HOPS_V6 => {
            sock.set_multicast_hops_v6(val_pointer.u32_value()?)?
        }
        MULTICAST_IF_V4 => {
            sock.set_multicast_if_v4(val_pointer.string_value()?)?
        }
        MULTICAST_IF_V6 => {
            sock.set_multicast_if_v6(val_pointer.u32_value()?)?
        }
        UNICAST_HOPS_V6 => {
            sock.set_unicast_hops_v6(val_pointer.u32_value()?)?
        }
        REUSE_ADDRESS => {
            sock.set_reuse_address(is_true!(state, val_pointer))?
        }
        REUSE_PORT => sock.set_reuse_port(is_true!(state, val_pointer))?,
        _ => {
            return Err(RuntimeError::Panic(format!(
                "The sock option {} is not valid",
                option
            )));
        }
    };

    Ok(val_pointer)
}

#[inline(always)]
pub fn socket_get_option(
    state: &RcState,
    process: &RcProcess,
    socket_pointer: ObjectPointer,
    option_pointer: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let sock = socket_pointer.socket_value()?;
    let option = option_pointer.integer_value()?;
    let result = match option {
        TTL => alloc_usize(state, process, sock.ttl()?),
        ONLY_V6 => alloc_bool(state, sock.only_v6()?),
        NODELAY => alloc_bool(state, sock.nodelay()?),
        BROADCAST => alloc_bool(state, sock.broadcast()?),
        LINGER => alloc_f64(state, process, sock.linger()?),
        RECV_SIZE => alloc_usize(state, process, sock.recv_buffer_size()?),
        SEND_SIZE => alloc_usize(state, process, sock.send_buffer_size()?),
        KEEPALIVE => alloc_f64(state, process, sock.keepalive()?),
        MULTICAST_LOOP_V4 => alloc_bool(state, sock.multicast_loop_v4()?),
        MULTICAST_LOOP_V6 => alloc_bool(state, sock.multicast_loop_v6()?),
        MULTICAST_TTL_V4 => {
            alloc_usize(state, process, sock.multicast_ttl_v4()?)
        }
        MULTICAST_HOPS_V6 => {
            alloc_usize(state, process, sock.multicast_hops_v6()?)
        }
        MULTICAST_IF_V4 => process.allocate(
            object_value::string(sock.multicast_if_v4()?),
            state.string_prototype,
        ),
        MULTICAST_IF_V6 => alloc_usize(state, process, sock.multicast_if_v6()?),
        UNICAST_HOPS_V6 => alloc_usize(state, process, sock.unicast_hops_v6()?),
        REUSE_ADDRESS => alloc_bool(state, sock.reuse_address()?),
        REUSE_PORT => alloc_bool(state, sock.reuse_port()?),
        _ => {
            return Err(RuntimeError::Panic(format!(
                "The sock option {} is not valid",
                option
            )));
        }
    };

    Ok(result)
}

#[inline(always)]
pub fn socket_shutdown(
    socket_ptr: ObjectPointer,
    mode_ptr: ObjectPointer,
) -> Result<(), RuntimeError> {
    let sock = socket_ptr.socket_value()?;
    let mode = mode_ptr.u8_value()?;

    sock.shutdown(mode)
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

fn alloc_usize(
    state: &RcState,
    process: &RcProcess,
    value: usize,
) -> ObjectPointer {
    process.allocate_usize(value, state.integer_prototype)
}

fn alloc_f64(
    state: &RcState,
    process: &RcProcess,
    value: f64,
) -> ObjectPointer {
    process.allocate(object_value::float(value), state.float_prototype)
}

fn alloc_bool(state: &RcState, value: bool) -> ObjectPointer {
    if value {
        state.true_object
    } else {
        state.false_object
    }
}

fn socket_prototype(state: &RcState, socket: &Socket) -> ObjectPointer {
    if socket.is_unix() {
        state.unix_socket_prototype
    } else {
        state.ip_socket_prototype
    }
}
