use crate::error::Error;
use std::time::Duration;
use ureq::{self, Agent, Error as HttpError, Response};

const TIMEOUT: u64 = 10;

pub(crate) fn get(url: &str) -> Result<Response, Error> {
    let agent = agent();

    match agent.get(url).call() {
        Ok(response) => Ok(response),
        Err(HttpError::Status(code, response)) => Err(Error::from(format!(
            "GET {} failed: HTTP {} {}",
            url,
            code,
            response.status_text()
        ))),
        Err(HttpError::Transport(err)) => {
            Err(Error::from(format!("GET {} failed: {}", url, err)))
        }
    }
}

fn agent() -> Agent {
    ureq::builder()
        .timeout_connect(Duration::from_secs(TIMEOUT))
        .timeout_read(Duration::from_secs(TIMEOUT))
        .user_agent(&format!("inko {}", env!("CARGO_PKG_VERSION")))
        .build()
}
