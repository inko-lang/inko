use crate::context;
use crate::mem::{Bool, ByteArray, Int, Nil, String as InkoString};
use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::result::Result;
use crate::scheduler::timeouts::Timeout;
use crate::socket::Socket;
use crate::state::State;
use std::io::{self, Write};

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

        socket.register(state, process, poll_id, interest)?;
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

fn new_address_pair(state: &State, addr: String, port: i64) -> Result {
    let addr = InkoString::alloc(state.string_class, addr);
    let port = Int::new(state.int_class, port);

    Result::ok_boxed((addr, port))
}

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_socket_new(
    proto: i64,
    kind: i64,
) -> Result {
    let sock = match proto {
        0 => Socket::ipv4(kind),
        1 => Socket::ipv6(kind),
        _ => Socket::unix(kind),
    };

    sock.map(Result::ok_boxed).unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_socket_write_string(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    input: *const InkoString,
    deadline: i64,
) -> Result {
    let state = &*state;

    blocking(state, process, &mut *socket, Interest::Write, deadline, |sock| {
        sock.write(InkoString::read(input).as_bytes())
    })
    .map(|v| Result::ok(Int::new(state.int_class, v as i64) as _))
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_socket_write_bytes(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    input: *mut ByteArray,
    deadline: i64,
) -> Result {
    let state = &*state;

    blocking(state, process, &mut *socket, Interest::Write, deadline, |sock| {
        sock.write(&(*input).value)
    })
    .map(|v| Result::ok(Int::new(state.int_class, v as i64) as _))
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
    .map(|size| Result::ok(Int::new(state.int_class, size as i64) as _))
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_listen(
    state: *const State,
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .listen(value as i32)
        .map(|_| Result::ok((*state).nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_bind(
    state: *const State,
    socket: *mut Socket,
    address: *const InkoString,
    port: i64,
) -> Result {
    // POSX states that bind(2) _can_ produce EINPROGRESS, but in practise it
    // seems no system out there actually does this.
    (*socket)
        .bind(InkoString::read(address), port as u16)
        .map(|_| Result::ok((*state).nil_singleton as _))
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
    .map(|_| Result::ok(state.nil_singleton as _))
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_accept(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    deadline: i64,
) -> Result {
    let state = &*state;

    blocking(state, process, &mut *socket, Interest::Read, deadline, |sock| {
        sock.accept()
    })
    .map(Result::ok_boxed)
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_receive_from(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    buffer: *mut ByteArray,
    amount: i64,
    deadline: i64,
) -> Result {
    let state = &*state;

    blocking(state, process, &mut *socket, Interest::Read, deadline, |sock| {
        sock.recv_from(&mut (*buffer).value, amount as _)
    })
    .map(|(addr, port)| new_address_pair(state, addr, port))
    .unwrap_or_else(Result::io_error)
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
    .map(|size| Result::ok(Int::new(state.int_class, size as i64) as _))
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
    .map(|size| Result::ok(Int::new(state.int_class, size as i64) as _))
    .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_shutdown_read(
    state: *const State,
    socket: *mut Socket,
) -> Result {
    (*socket)
        .shutdown_read()
        .map(|_| Result::ok((*state).nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_shutdown_write(
    state: *const State,
    socket: *mut Socket,
) -> Result {
    (*socket)
        .shutdown_write()
        .map(|_| Result::ok((*state).nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_shutdown_read_write(
    state: *const State,
    socket: *mut Socket,
) -> Result {
    (*socket)
        .shutdown_read_write()
        .map(|_| Result::ok((*state).nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_local_address(
    state: *const State,
    socket: *mut Socket,
) -> Result {
    (*socket)
        .local_address()
        .map(|(addr, port)| new_address_pair(&*state, addr, port))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_peer_address(
    state: *const State,
    socket: *mut Socket,
) -> Result {
    (*socket)
        .peer_address()
        .map(|(addr, port)| new_address_pair(&*state, addr, port))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_ttl(
    state: *const State,
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_ttl(value as _)
        .map(|_| Result::ok((*state).nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_only_v6(
    state: *const State,
    socket: *mut Socket,
    value: *const Bool,
) -> Result {
    let state = &*state;

    (*socket)
        .set_only_v6(value == state.true_singleton)
        .map(|_| Result::ok(state.nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_nodelay(
    state: *const State,
    socket: *mut Socket,
    value: *const Bool,
) -> Result {
    let state = &*state;

    (*socket)
        .set_nodelay(value == state.true_singleton)
        .map(|_| Result::ok(state.nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_broadcast(
    state: *const State,
    socket: *mut Socket,
    value: *const Bool,
) -> Result {
    let state = &*state;

    (*socket)
        .set_broadcast(value == state.true_singleton)
        .map(|_| Result::ok(state.nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_linger(
    state: *const State,
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_linger(value as _)
        .map(|_| Result::ok((*state).nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_recv_size(
    state: *const State,
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_recv_buffer_size(value as _)
        .map(|_| Result::ok((*state).nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_send_size(
    state: *const State,
    socket: *mut Socket,
    value: i64,
) -> Result {
    (*socket)
        .set_send_buffer_size(value as _)
        .map(|_| Result::ok((*state).nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_keepalive(
    state: *const State,
    socket: *mut Socket,
    value: *const Bool,
) -> Result {
    let state = &*state;

    (*socket)
        .set_keepalive(value == state.true_singleton)
        .map(|_| Result::ok(state.nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_reuse_address(
    state: *const State,
    socket: *mut Socket,
    value: *const Bool,
) -> Result {
    let state = &*state;

    (*socket)
        .set_reuse_address(value == state.true_singleton)
        .map(|_| Result::ok(state.nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_set_reuse_port(
    state: *const State,
    socket: *mut Socket,
    value: *const Bool,
) -> Result {
    let state = &*state;

    (*socket)
        .set_reuse_port(value == state.true_singleton)
        .map(|_| Result::ok(state.nil_singleton as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_try_clone(
    socket: *mut Socket,
) -> Result {
    (*socket).try_clone().map(Result::ok_boxed).unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_drop(
    state: *const State,
    socket: *mut Socket,
) -> *const Nil {
    drop(Box::from_raw(socket));
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_address_pair_address(
    pair: *const (*const InkoString, *const Int),
) -> *const InkoString {
    (*pair).0
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_address_pair_port(
    pair: *const (*const InkoString, *const Int),
) -> *const Int {
    (*pair).1
}

#[no_mangle]
pub unsafe extern "system" fn inko_socket_address_pair_drop(
    state: *const State,
    pair: *mut (*const InkoString, *const Int),
) -> *const Nil {
    drop(Box::from_raw(pair));
    (*state).nil_singleton
}
