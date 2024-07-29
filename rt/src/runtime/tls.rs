use crate::mem::{ByteArray, String as InkoString};
use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::result::Result;
use crate::runtime::helpers::poll;
use crate::rustls_platform_verifier::tls_config;
use crate::socket::{read_from, Socket};
use crate::state::State;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{
    ClientConfig, ClientConnection, Error as TlsError, RootCertStore,
    ServerConfig, ServerConnection, SideData, Stream,
};
use std::io::{self, Write};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// The error code produced when a TLS certificate is invalid.
const INVALID_CERT: isize = -1;

/// The error code produced when a TLS private key is invalid.
const INVALID_KEY: isize = -2;

unsafe fn run<
    C: Deref<Target = rustls::ConnectionCommon<S>> + DerefMut,
    R,
    S: SideData,
>(
    state: *const State,
    process: ProcessPointer,
    socket: *mut Socket,
    con: *mut C,
    deadline: i64,
    mut func: impl FnMut(&mut Stream<C, Socket>) -> io::Result<R>,
) -> io::Result<R> {
    let state = &*state;
    let mut stream = Stream::new(&mut *con, &mut *socket);

    loop {
        match func(&mut stream) {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                let interest = if stream.conn.wants_write() {
                    Interest::Write
                } else {
                    Interest::Read
                };

                poll(state, process, stream.sock, interest, deadline)?;
            }
            val => return val,
        }
    }
}

unsafe fn tls_close<
    C: Deref<Target = rustls::ConnectionCommon<S>> + DerefMut,
    S: SideData,
>(
    state: *const State,
    proc: ProcessPointer,
    sock: *mut Socket,
    con: *mut C,
    deadline: i64,
) -> io::Result<()> {
    (*con).send_close_notify();

    while (*con).wants_write() {
        run(state, proc, sock, con, deadline, |s| s.conn.write_tls(s.sock))?;
    }

    Ok(())
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_config_new() -> *mut ClientConfig
{
    Arc::into_raw(Arc::new(tls_config())) as *mut _
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_config_with_certificate(
    cert: *const ByteArray,
) -> Result {
    let mut store = RootCertStore::empty();
    let cert = CertificateDer::from((*cert).value.clone());

    if store.add(cert).is_err() {
        return Result::error(INVALID_CERT as _);
    }

    let conf = Arc::new(
        ClientConfig::builder()
            .with_root_certificates(store)
            .with_no_client_auth(),
    );

    Result::ok(Arc::into_raw(conf) as *mut _)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_config_clone(
    config: *const ClientConfig,
) -> *const ClientConfig {
    Arc::increment_strong_count(config);
    config
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_config_drop(
    config: *const ClientConfig,
) {
    drop(Arc::from_raw(config));
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_connection_new(
    config: *const ClientConfig,
    server: *const InkoString,
) -> Result {
    let name = match ServerName::try_from(InkoString::read(server)) {
        Ok(v) => v,
        Err(_) => return Result::none(),
    };

    Arc::increment_strong_count(config);

    // ClientConnection::new() _can_ in theory fail, but based on the source
    // code it seems this only happens when certain settings are adjusted, which
    // we don't allow at this time.
    let con = ClientConnection::new(Arc::from_raw(config), name)
        .expect("failed to set up the TLS client connection");

    Result::ok_boxed(con)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_connection_drop(
    state: *mut ClientConnection,
) {
    drop(Box::from_raw(state));
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_config_new(
    cert: *const ByteArray,
    key: *const ByteArray,
) -> Result {
    // CertificateDer/PrivateKeyDer either borrow a value or take an owned
    // value. We can't use borrows because we don't know if the Inko values
    // outlive the configuration, so we have to clone the bytes here.
    let chain = vec![CertificateDer::from((*cert).value.clone())];
    let Ok(key) = PrivateKeyDer::try_from((*key).value.clone()) else {
        return Result::error(INVALID_KEY as _);
    };
    let conf = match ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(chain, key)
    {
        Ok(v) => v,
        Err(
            TlsError::NoCertificatesPresented
            | TlsError::InvalidCertificate(_)
            | TlsError::UnsupportedNameType
            | TlsError::InvalidCertRevocationList(_),
        ) => return Result::error(INVALID_CERT as _),
        // For private key errors (and potentially others), rustls produces a
        // `Error::General`, and in the future possibly other errors. The "one
        // error type to rule them all" approach of rustls makes handling
        // specific cases painful, so we just treat all remaining errors as
        // private key errors. Given we already handle invalid certificates
        // above, this should be correct (enough).
        Err(_) => return Result::error(INVALID_KEY as _),
    };

    Result::ok(Arc::into_raw(Arc::new(conf)) as *mut _)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_config_clone(
    config: *const ServerConfig,
) -> *const ServerConfig {
    Arc::increment_strong_count(config);
    config
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_config_drop(
    config: *const ServerConfig,
) {
    drop(Arc::from_raw(config));
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_connection_new(
    config: *const ServerConfig,
) -> *mut ServerConnection {
    Arc::increment_strong_count(config);

    // ServerConnection::new() _can_ in theory fail, but based on the source
    // code it seems this only happens when certain settings are adjusted, which
    // we don't allow at this time.
    let con = ServerConnection::new(Arc::from_raw(config))
        .expect("failed to set up the TLS server connection");

    Box::into_raw(Box::new(con))
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_connection_drop(
    state: *mut ServerConnection,
) {
    drop(Box::from_raw(state));
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_write(
    state: *const State,
    proc: ProcessPointer,
    sock: *mut Socket,
    con: *mut ClientConnection,
    data: *mut u8,
    size: i64,
    deadline: i64,
) -> Result {
    let buf = std::slice::from_raw_parts(data, size as _);

    run(state, proc, sock, con, deadline, |s| s.write(buf))
        .map(|v| Result::ok(v as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_read(
    state: *const State,
    proc: ProcessPointer,
    sock: *mut Socket,
    con: *mut ClientConnection,
    buffer: *mut ByteArray,
    amount: i64,
    deadline: i64,
) -> Result {
    let buf = &mut (*buffer).value;
    let len = amount as usize;

    run(state, proc, sock, con, deadline, |s| read_from(s, buf, len))
        .map(|v| Result::ok(v as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_close(
    state: *const State,
    proc: ProcessPointer,
    sock: *mut Socket,
    con: *mut ClientConnection,
    deadline: i64,
) -> Result {
    tls_close(state, proc, sock, con, deadline)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_write(
    state: *const State,
    proc: ProcessPointer,
    sock: *mut Socket,
    con: *mut ServerConnection,
    data: *mut u8,
    size: i64,
    deadline: i64,
) -> Result {
    let buf = std::slice::from_raw_parts(data, size as _);

    run(state, proc, sock, con, deadline, |s| s.write(buf))
        .map(|v| Result::ok(v as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_read(
    state: *const State,
    proc: ProcessPointer,
    sock: *mut Socket,
    con: *mut ServerConnection,
    buffer: *mut ByteArray,
    amount: i64,
    deadline: i64,
) -> Result {
    let buf = &mut (*buffer).value;
    let len = amount as usize;

    run(state, proc, sock, con, deadline, |s| read_from(s, buf, len))
        .map(|v| Result::ok(v as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_close(
    state: *const State,
    proc: ProcessPointer,
    sock: *mut Socket,
    con: *mut ServerConnection,
    deadline: i64,
) -> Result {
    tls_close(state, proc, sock, con, deadline)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}
