//! Module for retrieving file timestamps as integers.
//!
//! Timestamps are returned as f64 values relative to the Unix epoch.
use std::fs::Metadata;
use std::io::Result as IOResult;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn duration_to_f64(duration: Duration) -> f64 {
    duration.as_secs() as f64 + (duration.subsec_nanos() as f64 / 1000000000.0)
}

/// Converts a `std::io::Result<SystemTime>` to a Unix timestamp.
pub fn system_time_to_unix_timestamp(
    time: IOResult<SystemTime>,
) -> Option<f64> {
    if let Ok(time) = time {
        match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => Some(duration_to_f64(duration)),
            Err(error) => Some(-duration_to_f64(error.duration())),
        }
    } else {
        None
    }
}

pub fn created_at(meta: Metadata) -> Option<f64> {
    system_time_to_unix_timestamp(meta.created())
}

pub fn modified_at(meta: Metadata) -> Option<f64> {
    system_time_to_unix_timestamp(meta.modified())
}

pub fn accessed_at(meta: Metadata) -> Option<f64> {
    system_time_to_unix_timestamp(meta.accessed())
}

#[cfg(test)]
mod tests {}
