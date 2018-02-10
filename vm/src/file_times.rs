//! Module for retrieving file timestamps as integers.
//!
//! Timestamps are returned as i64 values relative to the Unix epoch.
use std::fs::Metadata;
use std::i64;
use std::io::Result as IOResult;
use std::time::{SystemTime, UNIX_EPOCH};

macro_rules! in_i64_range {
    ($input: expr) => ({
        $input <= i64::MAX as u64
    })
}

/// Converts a `std::io::Result<SystemTime>` to a Unix timestamp.
pub fn system_time_to_unix_timestamp(
    time: IOResult<SystemTime>,
) -> Option<i64> {
    if let Ok(time) = time {
        match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                let secs = duration.as_secs();

                if in_i64_range!(secs) {
                    return Some(secs as i64);
                }
            }
            Err(error) => {
                let secs = error.duration().as_secs();

                if in_i64_range!(secs) {
                    return Some(-(secs as i64));
                }
            }
        }
    }

    None
}

pub fn created_at(meta: Metadata) -> Option<i64> {
    system_time_to_unix_timestamp(meta.created())
}

pub fn modified_at(meta: Metadata) -> Option<i64> {
    system_time_to_unix_timestamp(meta.modified())
}

pub fn accessed_at(meta: Metadata) -> Option<i64> {
    system_time_to_unix_timestamp(meta.accessed())
}

#[cfg(test)]
mod tests {}
