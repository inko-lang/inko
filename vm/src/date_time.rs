//! DateTime objects for the virtual machine and runtime.
use std::i64;
use std::time::SystemTime;
use time::OffsetDateTime;

/// Object for storing the local or UTC time.
#[derive(Clone)]
pub struct DateTime {
    time: OffsetDateTime,
}

/// The number of nanoseconds in a second.
const NANOS_PER_SEC: f64 = 1_000_000_000.0;

impl DateTime {
    /// Returns the current system time.
    pub fn now() -> Self {
        DateTime {
            time: OffsetDateTime::now_local(),
        }
    }

    /// Creates a `DateTime` from a `SystemTime` object.
    pub fn from_system_time(time: SystemTime) -> Self {
        DateTime { time: time.into() }
    }

    /// Returns the offset in seconds relative to UTC.
    pub fn utc_offset(&self) -> i64 {
        self.time.offset().as_seconds() as i64
    }

    /// Returns the seconds since the Unix epoch (including sub seconds).
    pub fn timestamp(&self) -> f64 {
        self.time.timestamp() as f64
            + (self.time.nanosecond() as f64 / NANOS_PER_SEC)
    }
}
