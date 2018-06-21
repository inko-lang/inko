//! Timer for measuring the elapsed time between two points.

use std::time::Instant;
use std::u64;

#[derive(Default)]
pub struct Timer {
    start: Option<Instant>,
    stop: Option<Instant>,
}

impl Timer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn now() -> Self {
        let mut timer = Timer::new();

        timer.start();

        timer
    }

    /// Returns the duration in nanoseconds.
    ///
    /// Since this method returns the time as a u64 care should be taken to
    /// ensure the duration is not long enough for the value to overflow.
    pub fn duration_nanosec(&self) -> u64 {
        if self.finished() {
            let start = self.start.unwrap();
            let stop = self.stop.unwrap();
            let duration = stop.duration_since(start);

            (duration.as_secs() * 1_000_000_000)
                + u64::from(duration.subsec_nanos())
        } else {
            0
        }
    }

    /// Returns the duration in milliseconds.
    pub fn duration_msec(&self) -> f64 {
        self.duration_nanosec() as f64 / 1_000_000.0
    }

    /// Returns the duration in seconds.
    pub fn duration_sec(&self) -> f64 {
        self.duration_nanosec() as f64 / 1_000_000_000.0
    }

    pub fn set_start_time(&mut self, time: Instant) {
        self.start = Some(time);
    }

    pub fn start(&mut self) {
        self.start = Some(Instant::now());
    }

    pub fn stop(&mut self) {
        self.stop = Some(Instant::now());
    }

    pub fn finished(&self) -> bool {
        self.start.is_some() && self.stop.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_new() {
        let timer = Timer::new();

        assert!(timer.start.is_none());
        assert!(timer.stop.is_none());
    }

    #[test]
    fn test_now() {
        let timer = Timer::now();

        assert!(timer.start.is_some());
    }

    #[test]
    fn test_duration_nanosec() {
        let mut timer = Timer::new();

        timer.start();
        thread::sleep(Duration::from_millis(10));
        timer.stop();

        assert!(timer.duration_nanosec() >= 10000000 as u64);
    }

    #[test]
    fn test_duration_msec() {
        let mut timer = Timer::new();

        timer.start();
        thread::sleep(Duration::from_millis(10));
        timer.stop();

        assert!(timer.duration_msec() >= 10.0);
    }

    #[test]
    fn test_duration_sec() {
        let mut timer = Timer::new();

        timer.start();
        thread::sleep(Duration::from_millis(10));
        timer.stop();

        assert!(timer.duration_sec() >= 0.01);
    }

    #[test]
    fn test_set_start_time() {
        let mut timer = Timer::new();

        timer.set_start_time(Instant::now());

        assert!(timer.start.is_some());
    }

    #[test]
    fn test_start() {
        let mut timer = Timer::new();

        timer.start();

        assert!(timer.start.is_some());
    }

    #[test]
    fn test_stop() {
        let mut timer = Timer::new();

        timer.stop();

        assert!(timer.stop.is_some());
    }

    #[test]
    fn test_finished() {
        let mut timer = Timer::new();

        assert_eq!(timer.finished(), false);

        timer.start();

        assert_eq!(timer.finished(), false);

        timer.stop();

        assert_eq!(timer.finished(), true);
    }
}
