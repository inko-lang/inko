//! Histograms for marked and available lines.
//!
//! A Histogram is used to track the distribution of marked and available lines
//! across Immix blocks. Each bin represents the number of holes with the values
//! representing the number of marked lines.
use crate::chunk::Chunk;

/// The minimum bin number that we care about when obtaining the most fragmented
/// bins.
///
/// Bins 0 and 1 are not interesting, because blocks with 0 or 1 holes are not
/// used for calculating fragmentation statistics.
pub const MINIMUM_BIN: usize = 2;

pub struct Histogram {
    // We use a u32 as this allows for 4 294 967 295 lines per bucket, which
    // equals roughly 512 GB of lines.
    values: Chunk<u32>,
}

impl Histogram {
    pub fn new(capacity: usize) -> Self {
        let values = Chunk::new(capacity);

        Histogram { values }
    }

    /// Increments a bin by the given value.
    ///
    /// Bounds checking is not performed, as the garbage collector never uses an
    /// out of bounds index.
    pub fn increment(&mut self, index: usize, value: u32) {
        debug_assert!(index < self.values.len());

        self.values[index] += value;
    }

    /// Returns the value for the given bin.
    ///
    /// Bounds checking is not performed, as the garbage collector never uses an
    /// out of bounds index.
    pub fn get(&self, index: usize) -> u32 {
        debug_assert!(
            index < self.values.len(),
            "index is {} but the length is {}",
            index,
            self.values.len()
        );

        self.values[index]
    }

    /// Removes all values from the histogram.
    pub fn reset(&mut self) {
        self.values.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let histo = Histogram::new(1);

        assert_eq!(histo.get(0), 0);
    }

    #[test]
    fn test_increment() {
        let mut histo = Histogram::new(1);

        histo.increment(0, 10);

        assert_eq!(histo.get(0), 10);
    }

    #[test]
    fn test_increment_successive() {
        let mut histo = Histogram::new(1);

        histo.increment(0, 5);
        histo.increment(0, 5);

        assert_eq!(histo.get(0), 10);
    }

    #[test]
    fn test_reset() {
        let mut histo = Histogram::new(1);

        histo.increment(0, 10);
        histo.reset();

        assert_eq!(histo.get(0), 0);
    }
}
