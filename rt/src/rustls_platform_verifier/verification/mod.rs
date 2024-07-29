use rustls::crypto::CryptoProvider;
use std::sync::Arc;

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "ios"),
    not(target_os = "tvos")
))]
mod others;

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "ios"),
    not(target_os = "tvos")
))]
pub use others::Verifier;

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
mod apple;

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
pub use apple::Verifier;

/// An EKU was invalid for the use case of verifying a server certificate.
///
/// This error is used primarily for tests.
#[derive(Debug, PartialEq)]
pub(crate) struct EkuError;

impl std::fmt::Display for EkuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("certificate had invalid extensions")
    }
}

impl std::error::Error for EkuError {}

// Log the certificate we are verifying so that we can try and find what may be wrong with it
// if we need to debug a user's situation.
fn log_server_cert(_end_entity: &rustls::pki_types::CertificateDer<'_>) {}

// Unknown certificate error shorthand. Used when we need to construct an "Other" certificate
// error with a platform specific error message.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
fn invalid_certificate(reason: impl Into<String>) -> rustls::Error {
    rustls::Error::InvalidCertificate(rustls::CertificateError::Other(
        rustls::OtherError(Arc::from(Box::from(reason.into()))),
    ))
}

impl Verifier {
    fn get_provider(&self) -> &Arc<CryptoProvider> {
        self.crypto_provider.get_or_init(|| {
            CryptoProvider::get_default()
                .expect("rustls default CryptoProvider not set")
                .clone()
        })
    }
}
