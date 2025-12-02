use crate::error::Error;
use rustls_graviola::default_provider;
use std::sync::Arc;
use std::time::Duration;
use ureq::http::Response;
use ureq::tls::{TlsConfig, TlsProvider};
use ureq::{self, Agent, Body};

const TIMEOUT: u64 = 10;

pub fn get(url: &str) -> Result<Response<Body>, Error> {
    let agent = agent();

    match agent.get(url).call() {
        Ok(response) => Ok(response),
        Err(err) => Err(Error::from(format!("GET {} failed: {}", url, err))),
    }
}

fn agent() -> Agent {
    let tls = TlsConfig::builder()
        .provider(TlsProvider::Rustls)
        .unversioned_rustls_crypto_provider(Arc::new(default_provider()))
        .build();

    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(TIMEOUT)))
        .user_agent(format!("inko {}", env!("CARGO_PKG_VERSION")))
        .tls_config(tls)
        .build()
        .into()
}
