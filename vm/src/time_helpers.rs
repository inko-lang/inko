//! Module for retrieving timestamps as integers.
//!
//! Timestamps are returned as f64 values relative to the Unix epoch.
use std::fs::Metadata;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::{self, Timespec, Tm};

const TM_START_YEAR: i64 = 1900;

/// Converts a `Duration` to an `f64`.
pub fn duration_to_f64(duration: Duration) -> f64 {
    duration.as_secs() as f64 + (duration.subsec_nanos() as f64 / 1000000000.0)
}

/// Converts a `std::io::Result<SystemTime>` to a Unix timestamp.
pub fn system_time_to_date_time(time: SystemTime) -> Tm {
    let (sec, nsec) = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            (duration.as_secs() as i64, duration.subsec_nanos() as i32)
        }
        Err(error) => {
            let duration = error.duration();

            (
                -(duration.as_secs() as i64),
                -(duration.subsec_nanos() as i32),
            )
        }
    };

    time::at(Timespec::new(sec, nsec))
}

/// Returns the creation time of a file.
pub fn created_at(meta: Metadata) -> Option<Tm> {
    meta.created()
        .ok()
        .map(|time| system_time_to_date_time(time))
}

/// Returns the modification time of a file.
pub fn modified_at(meta: Metadata) -> Option<Tm> {
    meta.modified()
        .ok()
        .map(|time| system_time_to_date_time(time))
}

/// Returns the access time of a file.
pub fn accessed_at(meta: Metadata) -> Option<Tm> {
    meta.accessed()
        .ok()
        .map(|time| system_time_to_date_time(time))
}

/// Returns the system time.
pub fn system_time() -> Tm {
    time::now()
}

/// Reads a field from a Tm value.
pub fn read_date_time_field(tm: &Tm, field: i64) -> Option<i64> {
    match field {
        0 => Some(tm.tm_sec as i64),
        1 => Some(tm.tm_nsec as i64),
        2 => Some(tm.tm_min as i64),
        3 => Some(tm.tm_hour as i64),
        4 => Some(tm.tm_mday as i64),
        5 => Some(tm.tm_mon as i64 + 1),
        6 => Some(tm.tm_year as i64 + TM_START_YEAR),
        7 => {
            let day = tm.tm_wday as i64;

            // Per ISO 8601 the first day of the week is Monday, not Sunday.
            if day == 0 {
                Some(7)
            } else {
                Some(day)
            }
        }
        8 => Some(tm.tm_yday as i64 + 1),
        9 => Some(tm.tm_isdst as i64),
        10 => Some(tm.tm_utcoff as i64),
        11 => Some(tm.to_timespec().sec),
        12 => Some(tm.to_timespec().nsec as i64),
        _ => None,
    }
}
