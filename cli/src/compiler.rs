//! Functions for interacting with the Inko compiler.
use crate::config;
use crate::error::Error;
use std::process::Command;

pub fn spawn(arguments: &[String]) -> Result<i32, Error> {
    let mut child = Command::new("ruby")
        .arg(config::compiler_bin())
        .args(arguments)
        .env("RUBYLIB", config::compiler_lib())
        .env("INKO_RUNTIME_PATH", config::runtime_path())
        .spawn()
        .map_err(|err| err.to_string())?;

    let status = child.wait().map(|s| s.code().unwrap_or(0))?;

    if status == 0 {
        Ok(0)
    } else {
        Err(Error::without_message(status))
    }
}
