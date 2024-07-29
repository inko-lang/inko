use rustls::ClientConfig;
use std::sync::Arc;

mod verification;
pub use verification::Verifier;

/// Creates and returns a `rustls` configuration that verifies TLS
/// certificates in the best way for the underlying OS platform, using
/// safe defaults for the `rustls` configuration.
///
/// # Example
///
/// This example shows how to use the custom verifier with the `reqwest` crate:
/// ```ignore
/// # use reqwest::ClientBuilder;
/// #[tokio::main]
/// async fn main() {
///     let client = ClientBuilder::new()
///         .use_preconfigured_tls(rustls_platform_verifier::tls_config())
///         .build()
///         .expect("nothing should fail");
///
///     let _response = client.get("https://example.com").send().await;
/// }
/// ```
///
/// **Important:** You must ensure that your `reqwest` version is using the same Rustls
/// version as this crate or it will panic when downcasting the `&dyn Any` verifier.
///
/// If you require more control over the rustls `ClientConfig`, you can
/// instantiate a [Verifier] with [Verifier::default] and then use it
/// with [`DangerousClientConfigBuilder::with_custom_certificate_verifier`][rustls::client::danger::DangerousClientConfigBuilder::with_custom_certificate_verifier].
///
/// Refer to the crate level documentation to see what platforms
/// are currently supported.
pub fn tls_config() -> ClientConfig {
    ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Verifier::new()))
        .with_no_client_auth()
}
