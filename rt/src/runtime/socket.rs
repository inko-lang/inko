use crate::context;
use crate::mem::{ByteArray, String as InkoString};
use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::result::{error_to_int, Result};
use crate::scheduler::timeouts::Timeout;
use crate::socket::Socket;
use crate::state::State;
use std::io::{self, Write};
use std::ptr::{drop_in_place, write};

#[repr(C)]
pub struct RawAddress {
    pub address: *const InkoString,
    pub port: i64,
}

impl RawAddress {
    fn new(state: &State, address: String, port: i64) -> RawAddress {
        RawAddress {
            address: InkoString::alloc(state.string_class, address),
            port,
        }
    }
}

fn blocking<T>(
    state: &State,
    mut process: ProcessPointer,
    socket: &mut Socket,
    interest: Interest,
    deadline: i64,
    mut func: impl FnMut(&mut Socket) -> io::Result<T>,
) -> io::Result<T> {
    match func(socket) {
        Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
        val => return val,
    }

    let poll_id = unsafe { process.thread() }.network_poller;

    // We must keep the process' state lock open until everything is registered,
    // otherwise a timeout thread may reschedule the process (i.e. the timeout
    // is very short) before we finish registering the socket with a poller.
    {
        let mut proc_state = process.state();

        // A deadline of -1 signals that we should wait indefinitely.
        if deadline >= 0 {
            let time = Timeout::until(deadline as u64);

            proc_state.waiting_for_io(Some(time.clone()));
            state.timeout_worker.suspend(process, time);
        } else {
            proc_state.waiting_for_io(None);
        }

        socket.register(state, process, poll_id, interest);
    }

    // Safety: the current thread is holding on to the process' run lock, so if
    // the process gets rescheduled onto a different thread, said thread won't
    // be able to use it until we finish this context switch.
    unsafe { context::switch(process) };

    if process.timeout_expired() {
        // The socket is still registered at this point, so we have to
        // deregister first. If we don't and suspend for another IO operation,
        // the poller could end up rescheduling the process multiple times (as
        // there are multiple events still in flight for the process).
        socket.deregister(state);
        return Err(io::Error::from(io::ErrorKind::TimedOut));
    }

    func(socket)
}

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_socket_new(
    proto: i64,
    kind: i64,
    out: *mut Socket,
) -> i64 {
    let sock = match proto {
        0 => Socket::ipv4(kind),
        1 => Socket::ipv6(kind),
        _ => Socket::unix(kind),
    };

    match sock {
        Ok(val) => {
            write(out, val);
            0
        }
        Err(err) => error_to_int(err),
    }
}

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_socket_write(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    data: *mut u8,
    size: i64,
    deadline: i64,
) -> Result {
    let state = &*state;
    let slice = std::slice::from_raw_parts(data, size as _);

    blocking(state, process, &mut *socket, Interest::Write, deadline, |sock| {
        sock.write(slice)
    })
    .map(|v| Result::ok(v as _))
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_read(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    buffer: *mut ByteArray,
    amount: i64,
    deadline: i64,
) -> Result {
    let state = &*state;

    blocking(state, process, &mut *socket, Interest::Read, deadline, |sock| {
        sock.read(&mut (*buffer).value, amount as usize)
    })
    .map(|size| Result::ok(size as _))
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_listen(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .listen(value as i32)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_bind(
    socket: *mut Socket,
    address: *const InkoString,
    port: i64,
) -> Result {
    // POSX states that bind(2) _can_ produce EINPROGRESS, but in practise it
    // seems no system out there actually does this.
    (*socket)
        .bind(InkoString::read(address), port as u16)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_connect(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    address: *const InkoString,
    port: i64,
    deadline: i64,
) -> Result {
    let state = &*state;

    blocking(state, process, &mut *socket, Interest::Write, deadline, |sock| {
        sock.connect(InkoString::read(address), port as u16)
    })
    .map(|_| Result::none())
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_accept(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    deadline: i64,
    out: *mut Socket,
) -> i64 {
    let res = blocking(
        &*state,
        process,
        &mut *socket,
        Interest::Read,
        deadline,
        |sock| sock.accept(),
    );

    match res {
        Ok(val) => {
            write(out, val);
            0
        }
        Err(err) => error_to_int(err),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_receive_from(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    buffer: *mut ByteArray,
    amount: i64,
    deadline: i64,
    out: *mut RawAddress,
) -> i64 {
    let state = &*state;
    let res = blocking(
        state,
        process,
        &mut *socket,
        Interest::Read,
        deadline,
        |sock| sock.recv_from(&mut (*buffer).value, amount as _),
    );

    match res {
        Ok((addr, port)) => {
            write(out, RawAddress::new(state, addr, port));
            0
        }
        Err(err) => error_to_int(err),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_send_bytes_to(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    buffer: *mut ByteArray,
    address: *const InkoString,
    port: i64,
    deadline: i64,
) -> Result {
    let state = &*state;
    let addr = InkoString::read(address);

    blocking(state, process, &mut *socket, Interest::Write, deadline, |sock| {
        sock.send_to(&(*buffer).value, addr, port as _)
    })
    .map(|size| Result::ok(size as _))
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_send_string_to(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    buffer: *const InkoString,
    address: *const InkoString,
    port: i64,
    deadline: i64,
) -> Result {
    let state = &*state;
    let addr = InkoString::read(address);

    blocking(state, process, &mut *socket, Interest::Write, deadline, |sock| {
        sock.send_to(InkoString::read(buffer).as_bytes(), addr, port as _)
    })
    .map(|size| Result::ok(size as _))
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_shutdown_read(
    socket: *mut Socket,
) -> Result {
    (*socket)
        .shutdown_read()
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_shutdown_write(
    socket: *mut Socket,
) -> Result {
    (*socket)
        .shutdown_write()
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_shutdown_read_write(
    socket: *mut Socket,
) -> Result {
    (*socket)
        .shutdown_read_write()
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_local_address(
    state: *const State,
    socket: *mut Socket,
    out: *mut RawAddress,
) -> i64 {
    match (*socket).local_address() {
        Ok((addr, port)) => {
            write(out, RawAddress::new(&*state, addr, port));
            0
        }
        Err(err) => error_to_int(err),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_peer_address(
    state: *const State,
    socket: *mut Socket,
    out: *mut RawAddress,
) -> i64 {
    match (*socket).peer_address() {
        Ok((addr, port)) => {
            write(out, RawAddress::new(&*state, addr, port));
            0
        }
        Err(err) => error_to_int(err),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_ttl(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_ttl(value as _)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_only_v6(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_only_v6(value == 1)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_nodelay(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_nodelay(value == 1)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_broadcast(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_broadcast(value == 1)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_linger(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_linger(value as _)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_recv_size(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_recv_buffer_size(value as _)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_send_size(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_send_buffer_size(value as _)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_keepalive(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_keepalive(value == 1)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_reuse_address(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_reuse_address(value == 1)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_reuse_port(
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_reuse_port(value == 1)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_try_clone(
    socket: *mut Socket,
    out: *mut Socket,
) -> i64 {
    match (*socket).try_clone() {
        Ok(val) => {
            write(out, val);
            0
        }
        Err(err) => error_to_int(err),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_drop(socket: *mut Socket) {
    drop_in_place(socket);
}
