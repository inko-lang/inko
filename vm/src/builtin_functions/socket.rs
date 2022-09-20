//! Functions for working with non-blocking sockets.
use crate::mem::{ByteArray, Float, Int, Pointer, String as InkoString};
use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::socket::Socket;
use crate::state::State;
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

pub(crate) fn socket_allocate_ipv4(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { Int::read(arguments[0]) };
    let socket = Socket::ipv4(kind)?;

    Ok(Pointer::boxed(socket))
}

pub(crate) fn socket_allocate_ipv6(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { Int::read(arguments[0]) };
    let socket = Socket::ipv6(kind)?;

    Ok(Pointer::boxed(socket))
}

pub(crate) fn socket_allocate_unix(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { Int::read(arguments[0]) };
    let socket = Socket::unix(kind)?;

    Ok(Pointer::boxed(socket))
}

pub(crate) fn socket_write_string(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let input = unsafe { InkoString::read(&arguments[1]).as_bytes() };
    let res = sock
        .write(input)
        .map(|size| Int::alloc(state.permanent_space.int_class(), size as i64))
        .map_err(RuntimeError::from);

    ret!(res, state, process, sock, Interest::Write)
}

pub(crate) fn socket_write_bytes(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let input = unsafe { arguments[1].get::<ByteArray>() }.value();
    let res = sock
        .write(input)
        .map(|size| Int::alloc(state.permanent_space.int_class(), size as i64))
        .map_err(RuntimeError::from);

    ret!(res, state, process, sock, Interest::Write)
}

pub(crate) fn socket_read(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let buffer = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let amount = unsafe { Int::read(arguments[2]) } as usize;

    let result = sock
        .read(buffer, amount)
        .map(|size| Int::alloc(state.permanent_space.int_class(), size as i64));

    ret!(result, state, process, sock, Interest::Read)
}

pub(crate) fn socket_listen(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let backlog = unsafe { Int::read(arguments[1]) } as i32;

    sock.listen(backlog)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_bind(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let addr = unsafe { InkoString::read(&arguments[1]) };
    let port = unsafe { Int::read(arguments[2]) } as u16;
    let result = sock.bind(addr, port).map(|_| Pointer::nil_singleton());

    ret!(result, state, process, sock, Interest::Read)
}

pub(crate) fn socket_connect(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let addr = unsafe { InkoString::read(&arguments[1]) };
    let port = unsafe { Int::read(arguments[2]) } as u16;
    let result = sock.connect(addr, port).map(|_| Pointer::nil_singleton());

    ret!(result, state, process, sock, Interest::Write)
}

pub(crate) fn socket_accept_ip(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let result = sock.accept().map(Pointer::boxed);

    ret!(result, state, process, sock, Interest::Read)
}

pub(crate) fn socket_accept_unix(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let result = sock.accept().map(Pointer::boxed);

    ret!(result, state, process, sock, Interest::Read)
}

pub(crate) fn socket_receive_from(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let buffer = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let amount = unsafe { Int::read(arguments[2]) } as usize;
    let result = sock
        .recv_from(buffer, amount)
        .map(|(addr, port)| allocate_address_pair(state, addr, port));

    ret!(result, state, process, sock, Interest::Read)
}

pub(crate) fn socket_send_bytes_to(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let buffer = unsafe { arguments[1].get::<ByteArray>() }.value();
    let address = unsafe { InkoString::read(&arguments[2]) };
    let port = unsafe { Int::read(arguments[3]) } as u16;
    let result = sock
        .send_to(buffer, address, port)
        .map(|size| Int::alloc(state.permanent_space.int_class(), size as i64));

    ret!(result, state, process, sock, Interest::Write)
}

pub(crate) fn socket_send_string_to(
    state: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let buffer = unsafe { InkoString::read(&arguments[1]).as_bytes() };
    let address = unsafe { InkoString::read(&arguments[2]) };
    let port = unsafe { Int::read(arguments[3]) } as u16;
    let result = sock
        .send_to(buffer, address, port)
        .map(|size| Int::alloc(state.permanent_space.int_class(), size as i64));

    ret!(result, state, process, sock, Interest::Write)
}

pub(crate) fn socket_shutdown_read(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.shutdown_read().map(|_| Pointer::nil_singleton())
}

pub(crate) fn socket_shutdown_write(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.shutdown_write().map(|_| Pointer::nil_singleton())
}

pub(crate) fn socket_shutdown_read_write(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.shutdown_read_write().map(|_| Pointer::nil_singleton())
}

pub(crate) fn socket_local_address(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get::<Socket>() };

    sock.local_address()
        .map(|(addr, port)| allocate_address_pair(state, addr, port))
}

pub(crate) fn socket_peer_address(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get::<Socket>() };

    sock.peer_address()
        .map(|(addr, port)| allocate_address_pair(state, addr, port))
}

pub(crate) fn socket_get_ttl(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { arguments[0].get::<Socket>() }.ttl()? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

pub(crate) fn socket_get_only_v6(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.only_v6()? {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn socket_get_nodelay(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get_mut::<Socket>() }.nodelay()? {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn socket_get_broadcast(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.broadcast()? {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn socket_get_linger(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { arguments[0].get::<Socket>() }.linger()?;

    Ok(Float::alloc(state.permanent_space.float_class(), value))
}

pub(crate) fn socket_get_recv_size(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(Int::alloc(
        state.permanent_space.int_class(),
        unsafe { arguments[0].get::<Socket>() }.recv_buffer_size()? as i64,
    ))
}

pub(crate) fn socket_get_send_size(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(Int::alloc(
        state.permanent_space.int_class(),
        unsafe { arguments[0].get::<Socket>() }.send_buffer_size()? as i64,
    ))
}

pub(crate) fn socket_get_keepalive(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { arguments[0].get::<Socket>() }.keepalive()?;

    if value {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn socket_get_reuse_address(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.reuse_address()? {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn socket_get_reuse_port(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.reuse_port()? {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn socket_set_ttl(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { Int::read(arguments[1]) } as u32;

    sock.set_ttl(value)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_only_v6(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.set_only_v6(arguments[1] == Pointer::true_singleton())?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_nodelay(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.set_nodelay(arguments[1] == Pointer::true_singleton())?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_broadcast(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.set_broadcast(arguments[1] == Pointer::true_singleton())?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_linger(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { Float::read(arguments[1]) };

    sock.set_linger(value)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_recv_size(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { Int::read(arguments[1]) } as usize;

    sock.set_recv_buffer_size(value)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_send_size(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { Int::read(arguments[1]) } as usize;

    sock.set_send_buffer_size(value)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_keepalive(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = arguments[1] == Pointer::true_singleton();

    sock.set_keepalive(value)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_reuse_address(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = arguments[1];

    sock.set_reuse_address(value == Pointer::true_singleton())?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_set_reuse_port(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = arguments[1];

    sock.set_reuse_port(value == Pointer::true_singleton())?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_try_clone(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get::<Socket>() };
    let clone = sock.try_clone()?;

    Ok(Pointer::boxed(clone))
}

pub(crate) fn socket_drop(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe { arguments[0].drop_boxed::<Socket>() };

    Ok(Pointer::nil_singleton())
}

pub(crate) fn socket_address_pair_address(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let pair = unsafe { arguments[0].get::<(Pointer, Pointer)>() };

    Ok(pair.0)
}

pub(crate) fn socket_address_pair_port(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let pair = unsafe { arguments[0].get::<(Pointer, Pointer)>() };

    Ok(pair.1)
}

pub(crate) fn socket_address_pair_drop(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe { Pointer::drop_boxed::<(Pointer, Pointer)>(arguments[0]) };

    Ok(Pointer::nil_singleton())
}

fn allocate_address_pair(state: &State, addr: String, port: i64) -> Pointer {
    let addr = InkoString::alloc(state.permanent_space.string_class(), addr);
    let port = Int::alloc(state.permanent_space.int_class(), port);

    Pointer::boxed((addr, port))
}
