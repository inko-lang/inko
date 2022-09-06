//! DateTime objects for the virtual machine and runtime.
use std::i64;
use std::time::SystemTime;
use time::OffsetDateTime;

/// Object for storing the local or UTC time.
pub(crate) struct DateTime {
    time: OffsetDateTime,
}

/// The number of nanoseconds in a second.
const NANOS_PER_SEC: f64 = 1_000_000_000.0;

impl DateTime {
    /// Returns the current system time.
    pub(crate) fn now() -> Self {
        DateTime { time: OffsetDateTime::now_local() }
    }

    /// Creates a `DateTime` from a `SystemTime` object.
    pub(crate) fn from_system_time(time: SystemTime) -> Self {
        DateTime { time: time.into() }
    }

    /// Returns the offset in seconds relative to UTC.
    pub(crate) fn utc_offset(&self) -> i64 {
        self.time.offset().as_seconds() as i64
    }

    /// Returns the seconds since the Unix epoch (including sub seconds).
    pub(crate) fn timestamp(&self) -> f64 {
        self.time.timestamp() as f64
            + (self.time.nanosecond() as f64 / NANOS_PER_SEC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // We can't make any guarantees about actual time values. The tests below
    // are just simple smoke tests to make sure the underlying
    // libraries/functions are working.

    #[test]
    fn test_utc_offset() {
        let date = DateTime::now();
        let offset = date.utc_offset();

        assert!(offset >= i64::MIN);
        assert!(offset <= i64::MAX);
    }

    #[test]
    fn test_timestamp() {
        let date = DateTime::now();
        let offset = date.timestamp();

        assert!(offset >= f64::MIN);
        assert!(offset <= f64::MAX);
    }
}
