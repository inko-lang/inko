//! Types for storing garbage collection statistics.
use std::ops::{Add, AddAssign};
use std::time::Duration;

/// Statistics produced by a single thread tracing objects.
pub struct TraceStatistics {
    /// The number of marked objects.
    pub marked: usize,

    /// The number of promoted objects.
    pub promoted: usize,

    /// The number of evacuated objects.
    pub evacuated: usize,
}

impl TraceStatistics {
    pub fn new() -> Self {
        TraceStatistics {
            marked: 0,
            promoted: 0,
            evacuated: 0,
        }
    }
}

impl Add for TraceStatistics {
    type Output = TraceStatistics;

    fn add(self, other: Self::Output) -> Self::Output {
        Self {
            marked: self.marked + other.marked,
            promoted: self.promoted + other.promoted,
            evacuated: self.evacuated + other.evacuated,
        }
    }
}

impl AddAssign for TraceStatistics {
    fn add_assign(&mut self, other: Self) {
        *self = Self {
            marked: self.marked + other.marked,
            promoted: self.promoted + other.promoted,
            evacuated: self.evacuated + other.evacuated,
        }
    }
}

/// Statistics about a single garbage collection.
pub struct CollectionStatistics {
    /// The total time spent garbage collecting.
    pub duration: Duration,

    /// The statistics produced by tracing objects.
    pub trace: TraceStatistics,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_statistics_add() {
        let mut stat1 = TraceStatistics::new();
        let mut stat2 = TraceStatistics::new();

        stat1.marked = 1;
        stat1.promoted = 1;
        stat1.evacuated = 1;

        stat2.marked = 1;
        stat2.promoted = 1;
        stat2.evacuated = 1;

        let stat3 = stat1 + stat2;

        assert_eq!(stat3.marked, 2);
        assert_eq!(stat3.promoted, 2);
        assert_eq!(stat3.evacuated, 2);
    }

    #[test]
    fn test_trace_statistics_add_assign() {
        let mut stat1 = TraceStatistics::new();
        let mut stat2 = TraceStatistics::new();

        stat1.marked = 1;
        stat1.promoted = 1;
        stat1.evacuated = 1;

        stat2.marked = 1;
        stat2.promoted = 1;
        stat2.evacuated = 1;

        stat1 += stat2;

        assert_eq!(stat1.marked, 2);
        assert_eq!(stat1.promoted, 2);
        assert_eq!(stat1.evacuated, 2);
    }
}
