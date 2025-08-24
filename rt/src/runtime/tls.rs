use crate::mem::PrimitiveString;
use crate::result::{self, Result};
use crate::rustls_platform_verifier::ConfigVerifierExt;
use crate::socket::Socket;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{
    ClientConfig, ClientConnection, Error as TlsError, RootCertStore,
    ServerConfig, ServerConnection, SideData, Stream,
};
use std::io::{self, Read, Write};
use std::ops::{Deref, DerefMut};
use std::slice;
use std::sync::Arc;

/// The error code produced when a TLS certificate is invalid.
const INVALID_CERT: isize = -1;

/// The error code produced when a TLS private key is invalid.
const INVALID_KEY: isize = -2;

type Callback = unsafe extern "system" fn(
    socket: *mut Socket,
    buffer: *mut u8,
    size: i64,
    deadline: i64,
) -> Result;

struct CallbackIo {
    /// The socket to read data from/write data to.
    socket: *mut Socket,

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

unsafe fn tls_close<
    C: Deref<Target = rustls::ConnectionCommon<S>> + DerefMut,
    S: SideData,
>(
    socket: *mut Socket,
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

#[no_mangle]
pub unsafe extern "system" fn inko_tls_client_config_new() -> *mut ClientConfig
{
    Arc::into_raw(Arc::new(ClientConfig::with_platform_verifier())) as *mut _
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
    server: PrimitiveString,
) -> Result {
    // ServerName::try_from supports both T and &T as input. We need a T here
    // as the Inko String input may not outlive the TLS client.
    let name = match ServerName::try_from(server.as_str().to_string()) {
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
    cert: *mut u8,
    cert_size: i64,
    key: *mut u8,
    key_size: i64,
) -> Result {
    let cert = slice::from_raw_parts(cert, cert_size as usize).to_vec();
    let key = slice::from_raw_parts(key, key_size as usize).to_vec();

    // CertificateDer/PrivateKeyDer either borrow a value or take an owned
    // value. We can't use borrows because we don't know if the Inko values
    // outlive the configuration, so we have to clone the bytes here.
    let chain = vec![CertificateDer::from(cert)];
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
    socket: *mut Socket,
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
    socket: *mut Socket,
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
    sock: *mut Socket,
    con: *mut ClientConnection,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    tls_close(sock, con, deadline, reader, writer)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_tls_server_write(
    socket: *mut Socket,
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
    socket: *mut Socket,
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
    sock: *mut Socket,
    con: *mut ServerConnection,
    deadline: i64,
    reader: Callback,
    writer: Callback,
) -> Result {
    tls_close(sock, con, deadline, reader, writer)
        .map(|_| Result::none())
        .unwrap_or_else(Result::io_error)
}
