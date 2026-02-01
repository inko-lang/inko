use crate::mem::PrimitiveString;
use crate::poll::Poll;
use crate::result::{self, Result};
use crate::rustls_platform_verifier::ConfigVerifierExt;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::server::{Accepted, Acceptor};
use rustls::{
    ClientConfig, ClientConnection, Error as TlsError, RootCertStore,
    ServerConfig, ServerConnection, SideData, Stream,
};
use std::io::{self, Read, Write};
use std::mem::forget;
use std::ops::{Deref, DerefMut};
use std::slice;
use std::sync::Arc;

/// The error code produced when a TLS certificate is invalid.
const INVALID_CERT: isize = -1;

/// The error code produced when a TLS private key is invalid.
const INVALID_KEY: isize = -2;

/// The client's hello message is invalid.
const INVALID_CLIENT_HELLO: isize = -3;

/// A client or server connection couldn't be established.
const INVALID_CONNECTION: isize = -4;

type Callback = unsafe extern "system" fn(
    socket: *mut Poll,
    buffer: *mut u8,
    size: i64,
    deadline: i64,
) -> Result;

struct CallbackIo {
    /// The socket to read data from/write data to.
    socket: *mut Poll,

    /// The callback function to use when data must be read.
    reader: Callback,

    /// The callback function to use when data must be written.
    writer: Callback,

    /// The deadline (in nanoseconds) after which operations will time out.
    deadline: i64,
}

impl Read for CallbackIo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let fun = self.reader;
        let res = unsafe {
            fun(self.socket, buf.as_mut_ptr(), buf.len() as i64, self.deadline)
        };

        if res.tag as i64 == result::OK {
            Ok(res.value as usize)
        } else {
            Err(io::Error::from_raw_os_error(res.value as i32))
        }
    }
}

impl Write for CallbackIo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let fun = self.writer;
        let res = unsafe {
            fun(self.socket, buf.as_ptr() as _, buf.len() as i64, self.deadline)
        };

        if res.tag as i64 == result::OK {
            Ok(res.value as usize)
        } else {
            Err(io::Error::from_raw_os_error(res.value as i32))
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

unsafe fn close<
    C: Deref<Target = rustls::ConnectionCommon<S>> + DerefMut,
    S: SideData,
>(
    socket: *mut Poll,
    con: *mut C,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> io::Result<()> {
    let mut io = CallbackIo { socket, reader, writer, deadline };
    let mut stream = Stream::new(&mut *con, &mut io);

    stream.conn.send_close_notify();

    while stream.conn.wants_write() {
        stream.conn.write_tls(&mut stream.sock)?;
    }

    Ok(())
}

unsafe fn complete_io<
    C: Deref<Target = rustls::ConnectionCommon<S>> + DerefMut,
    S: SideData,
>(
    socket: *mut Poll,
    con: *mut C,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> io::Result<()> {
    let mut io = CallbackIo { socket, reader, writer, deadline };
    let con = &mut *con;

    con.complete_io(&mut io).map(|_| ())
}

unsafe fn alpn_name<
    C: Deref<Target = rustls::ConnectionCommon<S>> + DerefMut,
    S: SideData,
>(
    state: *mut C,
) -> PrimitiveString {
    (&*state)
        .alpn_protocol()
        .map(|v| {
            PrimitiveString::owned(String::from_utf8_lossy(v).into_owned())
        })
        .unwrap_or(PrimitiveString::empty())
}

unsafe fn with_unique_config<T, F: FnOnce(&mut T)>(config: *const T, func: F) {
    let mut config = Arc::from_raw(config);

    if let Some(conf) = Arc::get_mut(&mut config) {
        func(conf);
    } else {
        // Due to how the standard library is implemented we should never reach
        // this branch, but it doesn't hurt to check _just_ in case.
        unreachable!("can't modify TLS configuration with multiple owners");
    }

    forget(config);
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_config_new() -> Result {
    if let Ok(v) = ClientConfig::with_platform_verifier() {
        Result::ok(Arc::into_raw(Arc::new(v)) as *mut _)
    } else {
        Result::none()
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_config_add_alpn(
    config: *const ClientConfig,
    name: *const u8,
    size: i64,
) {
    with_unique_config(config, |conf| {
        conf.alpn_protocols
            .push(slice::from_raw_parts(name, size as usize).to_vec());
    });
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_config_with_certificate(
    bytes: *mut u8,
    size: i64,
) -> Result {
    let mut store = RootCertStore::empty();
    let bytes = slice::from_raw_parts(bytes, size as usize);
    let cert = CertificateDer::from(bytes.to_vec());

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
pub unsafe extern "system" fn inko_tls_client_config_drop(
    config: *const ClientConfig,
) {
    drop(Arc::from_raw(config));
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_connection_new(
    config: *const ClientConfig,
    server: PrimitiveString,
    alpn: *mut PrimitiveString,
    alpn_size: i64,
) -> Result {
    // ServerName::try_from supports both T and &T as input. We need a T here
    // as the Inko String input may not outlive the TLS client.
    let name = match ServerName::try_from(server.as_str().to_string()) {
        Ok(v) => v,
        Err(_) => return Result::none(),
    };

    Arc::increment_strong_count(config);

    let config = Arc::from_raw(config);
    let con = if alpn_size > 0 {
        let alpn = slice::from_raw_parts(alpn, alpn_size as usize)
            .iter()
            .map(|s| s.as_str().as_bytes().to_vec())
            .collect();

        ClientConnection::new_with_alpn(config, name, alpn)
    } else {
        ClientConnection::new(config, name)
    };

    match con {
        Ok(v) => Result::ok_boxed(v),
        Err(_) => Result::error(INVALID_CONNECTION as _),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_connection_alpn(
    state: *mut ClientConnection,
) -> PrimitiveString {
    alpn_name(state)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_connection_drop(
    state: *mut ClientConnection,
) {
    drop(Box::from_raw(state));
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_config_new(
    chain: *mut Vec<Vec<u8>>,
    key: *mut u8,
    key_size: i64,
) -> Result {
    // CertificateDer/PrivateKeyDer either borrow a value or take an owned
    // value. We can't use borrows because we don't know if the Inko values
    // outlive the configuration, so we have to clone the bytes here.
    let chain =
        Box::from_raw(chain).into_iter().map(CertificateDer::from).collect();
    let key = slice::from_raw_parts(key, key_size as usize).to_vec();
    let Ok(key) = PrivateKeyDer::try_from(key) else {
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
pub unsafe extern "system" fn inko_tls_server_config_add_alpn(
    config: *const ServerConfig,
    name: *const u8,
    size: i64,
) {
    with_unique_config(config, |conf| {
        conf.alpn_protocols
            .push(slice::from_raw_parts(name, size as usize).to_vec());
    });
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
pub unsafe extern "system" fn inko_tls_server_connection_alpn(
    state: *mut ServerConnection,
) -> PrimitiveString {
    alpn_name(state)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_connection_server_name(
    state: *mut ServerConnection,
) -> PrimitiveString {
    (&*state)
        .server_name()
        .map(PrimitiveString::borrowed)
        .unwrap_or(PrimitiveString::empty())
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_connection_drop(
    state: *mut ServerConnection,
) {
    drop(Box::from_raw(state));
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_write(
    socket: *mut Poll,
    con: *mut ClientConnection,
    buffer: *mut u8,
    size: i64,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    let mut io = CallbackIo { socket, reader, writer, deadline };
    let buf = std::slice::from_raw_parts(buffer, size as _);

    Stream::new(&mut *con, &mut io)
        .write(buf)
        .map(|v| Result::ok(v as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_read(
    socket: *mut Poll,
    con: *mut ClientConnection,
    buffer: *mut u8,
    size: i64,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    let mut io = CallbackIo { socket, reader, writer, deadline };
    let buf = slice::from_raw_parts_mut(buffer, size as usize);

    Stream::new(&mut *con, &mut io)
        .read(buf)
        .map(|v| Result::ok(v as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_close(
    sock: *mut Poll,
    con: *mut ClientConnection,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    close(sock, con, deadline, reader, writer)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_write(
    socket: *mut Poll,
    con: *mut ServerConnection,
    buffer: *mut u8,
    size: i64,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    let mut io = CallbackIo { socket, reader, writer, deadline };
    let buf = std::slice::from_raw_parts(buffer, size as _);

    Stream::new(&mut *con, &mut io)
        .write(buf)
        .map(|v| Result::ok(v as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_read(
    socket: *mut Poll,
    con: *mut ServerConnection,
    buffer: *mut u8,
    size: i64,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    let mut io = CallbackIo { socket, reader, writer, deadline };
    let buf = slice::from_raw_parts_mut(buffer, size as usize);

    Stream::new(&mut *con, &mut io)
        .read(buf)
        .map(|v| Result::ok(v as _))
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_close(
    sock: *mut Poll,
    con: *mut ServerConnection,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    close(sock, con, deadline, reader, writer)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_complete_io(
    socket: *mut Poll,
    con: *mut ServerConnection,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    match complete_io(socket, con, deadline, reader, writer) {
        Ok(_) => Result::none(),
        Err(e) => Result::io_error(e),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_complete_io(
    socket: *mut Poll,
    con: *mut ClientConnection,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    match complete_io(socket, con, deadline, reader, writer) {
        Ok(_) => Result::none(),
        Err(e) => {
            match e.get_ref().and_then(|e| e.downcast_ref::<TlsError>()) {
                Some(
                    TlsError::NoCertificatesPresented
                    | TlsError::InvalidCertificate(_)
                    | TlsError::UnsupportedNameType
                    | TlsError::InvalidCertRevocationList(_),
                ) => Result::error(INVALID_CERT as _),
                _ => Result::io_error(e),
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_pending_server_new(
    socket: *mut Poll,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    let mut io = CallbackIo { socket, reader, writer, deadline };
    let mut acceptor = Acceptor::default();

    loop {
        if let Err(e) = acceptor.read_tls(&mut io) {
            return Result::io_error(e);
        }

        let accepted = match acceptor.accept() {
            Ok(Some(v)) => v,
            Ok(_) => continue,
            Err((_, mut alert)) => {
                // If writing the alert fails that's fine because it can fail
                // for all sorts of reasons we don't care about (e.g. a network
                // error) and we don't want it to overwrite the error below.
                let _ = alert.write_all(&mut io);

                // rustls errors are opaque and it's not clear what exact errors
                // we may encounter here, so we just generalize all of them as a
                // "client hello is invalid" error.
                return Result::error(INVALID_CLIENT_HELLO as _);
            }
        };

        return Result::ok_boxed(accepted);
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_pending_server_name(
    accepted: *mut Accepted,
) -> PrimitiveString {
    let accepted = &*accepted;

    accepted
        .client_hello()
        .server_name()
        .map(PrimitiveString::borrowed)
        .unwrap_or(PrimitiveString::empty())
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_pending_server_into_server_connection(
    accepted: *mut Accepted,
    config: *const ServerConfig,
    socket: *mut Poll,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    Arc::increment_strong_count(config);

    let mut io = CallbackIo { socket, reader, writer, deadline };
    let config = Arc::from_raw(config);
    let acc = Box::from_raw(accepted);

    match acc.into_connection(config) {
        Ok(v) => Result::ok_boxed(v),
        Err((_, mut alert)) => {
            let _ = alert.write_all(&mut io);

            Result::error(INVALID_CLIENT_HELLO as _)
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_pending_server_drop(
    accepted: *mut Accepted,
) {
    drop(Box::from_raw(accepted));
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_cert_chain_new() -> *mut Vec<Vec<u8>> {
    Box::into_raw(Box::new(Vec::new()))
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_cert_chain_push(
    chain: *mut Vec<Vec<u8>>,
    bytes: *mut u8,
    size: i64,
) {
    let slice = slice::from_raw_parts(bytes, size as usize).to_vec();

    (&mut *chain).push(slice.to_vec());
}
