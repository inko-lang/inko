use crate::error::Error;
use std::time::Duration;
use ureq::http::Response;
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
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(TIMEOUT)))
        .user_agent(format!("inko {}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}
