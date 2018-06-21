//! DateTime objects for the virtual machine and runtime.

use std::i32;
use std::i64;
use std::time::{SystemTime, UNIX_EPOCH};
use time::{self, Timespec, Tm};

/// Object for storing the local or UTC time.
#[derive(Clone)]
pub struct DateTime {
    time: Tm,
}

/// The number of nanoseconds in a second.
const NANOS_PER_SEC: f64 = 1_000_000_000.0;

impl DateTime {
    pub fn new(tm: Tm) -> Self {
        DateTime { time: tm }
    }

    /// Returns the current system time.
    pub fn now() -> Self {
        DateTime::new(time::now())
    }

    /// Returns the current time in UTC.
    pub fn now_utc() -> Self {
        DateTime::new(time::now_utc())
    }

    /// Creates a `DateTime` from a `SystemTime` object.
    pub fn from_system_time(time: SystemTime) -> Self {
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

        DateTime::new(time::at(Timespec::new(sec, nsec)))
    }

    /// Returns the nanoseconds after the second.
    pub fn nanoseconds(&self) -> i64 {
        i64::from(self.time.tm_nsec)
    }

    /// Returns a flag indicating if Daylight Saving Time is active.
    pub fn dst_active(&self) -> bool {
        self.time.tm_isdst == 1
    }

    /// Returns the offset in seconds relative to UTC.
    pub fn utc_offset(&self) -> i64 {
        i64::from(self.time.tm_utcoff)
    }

    /// Returns the seconds since the Unix epoch (including sub seconds).
    pub fn timestamp(&self) -> f64 {
        self.time.to_timespec().sec as f64
            + self.nanoseconds() as f64 / NANOS_PER_SEC
    }
}
