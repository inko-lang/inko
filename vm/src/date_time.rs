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

/// The year the "tm_year" values are relative to.
const TM_START_YEAR: i64 = 1900;

/// The number of nanoseconds in a second.
const NANOS_PER_SEC: f64 = 1_000_000_000.0;

fn seconds_to_timespec(secs: f64) -> Option<Timespec> {
    if secs > (i64::MAX as f64) {
        return None;
    }

    let nanos = secs.fract() * NANOS_PER_SEC;

    if nanos <= (i32::MAX as f64) {
        let timespec = Timespec::new(secs as i64, nanos as i32);

        Some(timespec)
    } else {
        None
    }
}

impl DateTime {
    /// Returns the current system time.
    pub fn now() -> Self {
        DateTime { time: time::now() }
    }

    /// Returns the current time in UTC.
    pub fn now_utc() -> Self {
        DateTime {
            time: time::now_utc(),
        }
    }

    /// Returns a `DateTime` starting at the given number of seconds since the
    /// Unix epoch, using the local timezone.
    pub fn from_seconds(secs: f64) -> Option<Self> {
        seconds_to_timespec(secs).map(|ts| DateTime { time: time::at(ts) })
    }

    /// Returns a `DateTime` starting at the given number of seconds since the
    /// Unix epoch, using UTC as the timezone.
    pub fn from_seconds_utc(secs: f64) -> Option<Self> {
        seconds_to_timespec(secs).map(|ts| DateTime {
            time: time::at_utc(ts),
        })
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

        DateTime {
            time: time::at(Timespec::new(sec, nsec)),
        }
    }

    /// Parses a String into a DateTime using a specific format.
    pub fn parse(string: &str, format: &str) -> Result<DateTime, String> {
        time::strptime(string, format)
            .map(|tm| DateTime { time: tm })
            .map_err(|err| {
                format!("The string {:?} could not be parsed using format {:?}: {:}", string, format, err)
            })
    }

    /// Returns the second of the minute from 0 to 60.
    pub fn second(&self) -> i64 {
        self.time.tm_sec as i64
    }

    /// Returns the nanoseconds after the second.
    pub fn nanoseconds(&self) -> i64 {
        self.time.tm_nsec as i64
    }

    /// Returns the minute after the hour from 0 to 59.
    pub fn minute(&self) -> i64 {
        self.time.tm_min as i64
    }

    /// Returns the hour of the day from 0 to 23.
    pub fn hour(&self) -> i64 {
        self.time.tm_hour as i64
    }

    /// Returns the day of the month from 1 to 31.
    pub fn day(&self) -> i64 {
        self.time.tm_mday as i64
    }

    /// Returns the month of the year from 1 to 12.
    pub fn month(&self) -> i64 {
        self.time.tm_mon as i64 + 1
    }

    /// Returns the year.
    pub fn year(&self) -> i64 {
        self.time.tm_year as i64 + TM_START_YEAR
    }

    /// Returns the day of the week, from 1 to 7.
    ///
    /// Per ISO 8601 the first day of the week is Monday, not Sunday.
    pub fn day_of_week(&self) -> i64 {
        let day = self.time.tm_wday as i64;

        if day == 0 {
            7
        } else {
            day
        }
    }

    /// Returns the number of days since January 1st from 1 to 366.
    pub fn day_of_year(&self) -> i64 {
        self.time.tm_yday as i64 + 1
    }

    /// Returns a flag indicating if Daylight Saving Time is active.
    ///
    /// This method returns 1 if DST is active, 0 otherwise.
    pub fn dst_active(&self) -> i64 {
        self.time.tm_isdst as i64
    }

    /// Returns the offset in seconds relative to UTC.
    pub fn utc_offset(&self) -> i64 {
        self.time.tm_utcoff as i64
    }

    /// Returns the seconds since the Unix epoch.
    pub fn seconds_since_epoch(&self) -> i64 {
        self.time.to_timespec().sec
    }

    /// Returns the value of a field based on the given index.
    pub fn get(&self, field: i64) -> Option<i64> {
        match field {
            0 => Some(self.second()),
            1 => Some(self.nanoseconds()),
            2 => Some(self.minute()),
            3 => Some(self.hour()),
            4 => Some(self.day()),
            5 => Some(self.month()),
            6 => Some(self.year()),
            7 => Some(self.day_of_week()),
            8 => Some(self.day_of_year()),
            9 => Some(self.dst_active()),
            10 => Some(self.utc_offset()),
            11 => Some(self.seconds_since_epoch()),
            _ => None,
        }
    }

    /// Formats the DateTime as a String using the given format.
    pub fn format(&self, format: &String) -> Result<String, String> {
        time::strftime(format, &self.time).map_err(|err| {
            format!("The time format {:?} is invalid: {}", format, err)
        })
    }
}
