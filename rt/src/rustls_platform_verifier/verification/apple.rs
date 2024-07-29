use super::log_server_cert;
use crate::rustls_platform_verifier::verification::invalid_certificate;
use core_foundation::date::CFDate;
use core_foundation_sys::date::kCFAbsoluteTimeIntervalSince1970;
use once_cell::sync::OnceCell;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerifier};
use rustls::crypto::{
    verify_tls12_signature, verify_tls13_signature, CryptoProvider,
};
use rustls::pki_types;
use rustls::{
    CertificateError, DigitallySignedStruct, Error as TlsError, OtherError,
    SignatureScheme,
};
use security_framework::{
    certificate::SecCertificate, policy::SecPolicy,
    secure_transport::SslProtocolSide, trust::SecTrust,
};
use std::sync::Arc;

use crate::process::ProcessPointer;
use crate::scheduler::process::CURRENT_PROCESS;

mod errors {
    pub(super) use security_framework_sys::base::{
        errSecCertificateRevoked, errSecCreateChainFailed,
        errSecHostNameMismatch, errSecInvalidExtendedKeyUsage,
    };
}

#[allow(clippy::as_conversions)]
fn system_time_to_cfdate(
    time: pki_types::UnixTime,
) -> Result<CFDate, TlsError> {
    // SAFETY: The interval is defined by macOS externally, but is always present and never modified at runtime
    // since its a global variable.
    //
    // See https://developer.apple.com/documentation/corefoundation/kcfabsolutetimeintervalsince1970.
    let unix_adjustment = unsafe { kCFAbsoluteTimeIntervalSince1970 as u64 };

    // Convert a system timestamp based off the UNIX epoch into the
    // Apple epoch used by all `CFAbsoluteTime` values.
    // Subtracting Durations with sub() will panic on overflow
    time.as_secs()
        .checked_sub(unix_adjustment)
        .ok_or(TlsError::FailedToGetCurrentTime)
        .map(|epoch| CFDate::new(epoch as f64))
}

/// A TLS certificate verifier that utilizes the Apple platform certificate facilities.
#[derive(Debug)]
pub struct Verifier {
    pub(super) crypto_provider: OnceCell<Arc<CryptoProvider>>,
}

impl Verifier {
    /// Creates a new instance of a TLS certificate verifier that utilizes the macOS certificate
    /// facilities.
    ///
    /// A [`CryptoProvider`] must be set with
    /// [`set_provider`][Verifier::set_provider]/[`with_provider`][Verifier::with_provider] or
    /// [`CryptoProvider::install_default`] before the verifier can be used.
    pub fn new() -> Self {
        Self { crypto_provider: OnceCell::new() }
    }

    fn verify_certificate(
        &self,
        end_entity: &pki_types::CertificateDer<'_>,
        intermediates: &[pki_types::CertificateDer<'_>],
        server_name: &str,
        ocsp_response: Option<&[u8]>,
        now: pki_types::UnixTime,
    ) -> Result<(), TlsError> {
        let certificates: Vec<SecCertificate> =
            std::iter::once(end_entity.as_ref())
                .chain(intermediates.iter().map(|cert| cert.as_ref()))
                .map(|cert| {
                    SecCertificate::from_der(cert).map_err(|_| {
                        TlsError::InvalidCertificate(
                            CertificateError::BadEncoding,
                        )
                    })
                })
                .collect::<Result<Vec<SecCertificate>, _>>()?;

        // Create our verification policy suitable for verifying TLS chains.
        // This uses the "default" verification engine and parameters, the same as Windows.
        //
        // The protocol side should be set to `server` for a client to verify server TLS
        // certificates.
        //
        // The server name will be required to match what the end-entity certificate reports
        //
        // Ref: https://developer.apple.com/documentation/security/1392592-secpolicycreatessl
        let policy =
            SecPolicy::create_ssl(SslProtocolSide::SERVER, Some(server_name));

        // Create our trust evaluation context/chain.
        //
        // Apple requires that the certificate to be verified is always first in the array, and we
        // always place the end-entity certificate at the start.
        //
        // Ref: https://developer.apple.com/documentation/security/1401555-sectrustcreatewithcertificates
        let mut trust_evaluation =
            SecTrust::create_with_certificates(&certificates, &[policy])
                .map_err(|e| TlsError::General(e.to_string()))?;

        // Tell the system that we want to consider the certificates evaluation at the point
        // in time that `rustls` provided.
        let now = system_time_to_cfdate(now)?;
        trust_evaluation
            .set_trust_verify_date(&now)
            .map_err(|e| invalid_certificate(e.to_string()))?;

        // If we have OCSP response data, make sure the system makes use of it.
        if let Some(ocsp_response) = ocsp_response {
            trust_evaluation
                .set_trust_ocsp_response(std::iter::once(ocsp_response))
                .map_err(|e| invalid_certificate(e.to_string()))?;
        }

        // Safety: well, technically none, but due to the way the runtime uses
        // the verifier this should never misbehave.
        let process = unsafe { ProcessPointer::new(CURRENT_PROCESS.get()) };
        let trust_error =
            match process.blocking(|| trust_evaluation.evaluate_with_error()) {
                Ok(()) => return Ok(()),
                Err(e) => e,
            };

        let err_code = trust_error.code();

        let err = err_code
            .try_into()
            .map_err(|_| ())
            .and_then(|code| {
                // Only map the errors we need for tests.
                match code {
                    errors::errSecHostNameMismatch => Ok(TlsError::InvalidCertificate(
                        CertificateError::NotValidForName,
                    )),
                    errors::errSecCreateChainFailed => Ok(TlsError::InvalidCertificate(
                        CertificateError::UnknownIssuer,
                    )),
                    errors::errSecInvalidExtendedKeyUsage => Ok(TlsError::InvalidCertificate(
                        CertificateError::Other(OtherError(std::sync::Arc::new(super::EkuError))),
                    )),
                    errors::errSecCertificateRevoked => {
                        Ok(TlsError::InvalidCertificate(CertificateError::Revoked))
                    }
                    _ => Err(()),
                }
            })
            // Fallback to an error containing the description and specific error code so that
            // the exact error cause can be looked up easily.
            .unwrap_or_else(|_| invalid_certificate(format!("{}: {}", trust_error, err_code)));

        Err(err)
    }
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &pki_types::CertificateDer<'_>,
        intermediates: &[pki_types::CertificateDer<'_>],
        server_name: &pki_types::ServerName,
        ocsp_response: &[u8],
        now: pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, TlsError> {
        log_server_cert(end_entity);

        // Convert IP addresses to name strings to ensure match check on leaf certificate.
        // Ref: https://developer.apple.com/documentation/security/1392592-secpolicycreatessl
        let server = server_name.to_str();

        let ocsp_data =
            if !ocsp_response.is_empty() { Some(ocsp_response) } else { None };

        match self.verify_certificate(
            end_entity,
            intermediates,
            &server,
            ocsp_data,
            now,
        ) {
            Ok(()) => {
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            }
            Err(e) => {
                // This error only tells us what the system errored with, so it doesn't leak anything
                // sensitive.
                Err(e)
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &pki_types::CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.get_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &pki_types::CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.get_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.get_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}
