//! Module for retrieving timestamps as integers.
//!
//! Timestamps are returned as f64 values relative to the Unix epoch.
use std::fs::Metadata;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Converts a `Duration` to an `f64`.
pub fn duration_to_f64(duration: Duration) -> f64 {
    duration.as_secs() as f64 + (duration.subsec_nanos() as f64 / 1000000000.0)
}

/// Converts a `std::io::Result<SystemTime>` to a Unix timestamp.
pub fn system_time_to_unix_timestamp(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration_to_f64(duration),
        Err(error) => -duration_to_f64(error.duration()),
    }
}

/// Returns the creation time of a file.
pub fn created_at(meta: Metadata) -> Option<f64> {
    meta.created()
        .ok()
        .map(|time| system_time_to_unix_timestamp(time))
}

/// Returns the modification time of a file.
pub fn modified_at(meta: Metadata) -> Option<f64> {
    meta.modified()
        .ok()
        .map(|time| system_time_to_unix_timestamp(time))
}

/// Returns the access time of a file.
pub fn accessed_at(meta: Metadata) -> Option<f64> {
    meta.accessed()
        .ok()
        .map(|time| system_time_to_unix_timestamp(time))
}

/// Returns the current system time.
pub fn system_time() -> f64 {
    system_time_to_unix_timestamp(SystemTime::now())
}
