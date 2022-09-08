//! Functions for system and monotonic clocks.
use crate::mem::{Float, Int, Pointer};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::state::State;
use std::mem::MaybeUninit;

#[cfg(not(windows))]
fn utc() -> f64 {
    unsafe {
        let mut ts = MaybeUninit::uninit();

        if libc::clock_gettime(libc::CLOCK_REALTIME, ts.as_mut_ptr()) != 0 {
            panic!("clock_gettime() failed");
        }

        let ts = ts.assume_init();

        ts.tv_sec as f64 + (ts.tv_nsec as f64 / 1_000_000_000.0)
    }
}

#[cfg(not(windows))]
fn offset() -> i64 {
    unsafe {
        extern "C" {
            fn tzset();
        }

        let ts = {
            let mut ts = MaybeUninit::uninit();

            if libc::clock_gettime(libc::CLOCK_REALTIME, ts.as_mut_ptr()) != 0 {
                panic!("clock_gettime() failed");
            }

            ts.assume_init()
        };

        let mut tm = MaybeUninit::uninit();

        // localtime_r() doesn't necessarily call tzset() for us.
        tzset();

        // While localtime_r() may call setenv() internally, this is not a
        // problem as Inko caches environment variables upon startup. If an FFI
        // call ends up racing with the setenv() call, that's a problem for the
        // FFI code.
        if libc::localtime_r(&ts.tv_sec, tm.as_mut_ptr()).is_null() {
            panic!("localtime_r() failed");
        }

        tm.assume_init().tm_gmtoff
    }
}

#[cfg(windows)]
fn utc() -> f64 {
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

    unsafe {
        let ft = {
            let mut ft = MaybeUninit::uninit();

            GetSystemTimeAsFileTime(ft.as_mut_ptr());
            ft.assume_init()
        };

        let intervals_per_sec = 10_000_000;
        let intervals_to_unix = 11_644_473_600 * intervals_per_sec;
        let win_time =
            i64::from(ft.dwHighDateTime) << 32 | i64::from(ft.dwLowDateTime);

        (win_time - intervals_to_unix) as f64 / intervals_per_sec as f64
    }
}

#[cfg(windows)]
fn offset() -> i64 {
    use windows_sys::Win32::System::SystemServices::{
        TIME_ZONE_ID_DAYLIGHT, TIME_ZONE_ID_STANDARD, TIME_ZONE_ID_UNKNOWN,
    };
    use windows_sys::Win32::System::Time::GetTimeZoneInformation;

    unsafe {
        let mut tz = MaybeUninit::uninit();
        let bias = match GetTimeZoneInformation(tz.as_mut_ptr()) {
            TIME_ZONE_ID_UNKNOWN => tz.assume_init().Bias as i64,
            TIME_ZONE_ID_STANDARD => {
                let tz = tz.assume_init();

                tz.Bias as i64 + tz.StandardBias as i64
            }
            TIME_ZONE_ID_DAYLIGHT => {
                let tz = tz.assume_init();

                tz.Bias as i64 + tz.DaylightBias as i64
            }
            _ => 0,
        };

        // The bias (in minutes) is the result of `UTC - local time`, so if
        // you're ahead of UTC the bias is negative.
        bias * -60
    }
}

pub(crate) fn time_monotonic(
    state: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let duration = state.start_time.elapsed();
    let seconds = duration.as_secs_f64();

    Ok(Float::alloc(state.permanent_space.float_class(), seconds))
}

pub(crate) fn time_system(
    state: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(Float::alloc(state.permanent_space.float_class(), utc()))
}

pub(crate) fn time_system_offset(
    state: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(Int::alloc(state.permanent_space.int_class(), offset()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_utc() {
        let expected =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
        let given = utc();

        // We can't assert equality, for there may be time between the two
        // function calls. We also can't assert the utc() time is greater in the
        // event of clock changes. Instead we just assert the two times are
        // within 5 seconds of each other, which should be sufficient.
        assert!((given - expected).abs() < 5.0);
    }
}
