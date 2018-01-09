//! Histograms for marked and available lines.
//!
//! A Histogram is used to track the distribution of marked and available lines
//! across Immix blocks. Each bin represents the number of holes with the values
//! representing the number of marked lines.

const DEFAULT_VALUE: usize = 0;

pub struct Histogram {
    values: Vec<usize>,
}

/// Iterator for traversing the most fragmented bins in a histogram.
pub struct HistogramIterator<'a> {
    histogram: &'a Histogram,
    index: isize,
}

impl Histogram {
    pub fn new(capacity: usize) -> Self {
        Histogram {
            values: vec![DEFAULT_VALUE; capacity],
        }
    }

    /// Increments a bin by the given value.
    #[inline]
    pub fn increment(&mut self, index: usize, value: usize) {
        self.values[index] += value;
    }

    /// Returns the value for the given bin.
    #[inline]
    pub fn get(&self, index: usize) -> usize {
        self.values[index]
    }

    /// Returns the most fragmented bin.
    pub fn most_fragmented_bin(&self) -> usize {
        for (bin, value) in self.values.iter().enumerate().rev() {
            if *value > DEFAULT_VALUE {
                return bin;
            }
        }

        0
    }

    /// Returns an iterator for traversing the most fragmented bins in
    /// descending order.
    pub fn iter<'a>(&'a self) -> HistogramIterator<'a> {
        HistogramIterator {
            index: self.most_fragmented_bin() as isize,
            histogram: self,
        }
    }

    /// Removes all values from the histogram.
    pub fn reset(&mut self) {
        for index in 0..self.values.len() {
            self.values[index] = DEFAULT_VALUE;
        }
    }
}

impl<'a> Iterator for HistogramIterator<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.index >= 0 {
            let index = self.index as usize;
            let value = self.histogram.get(index as usize);

            self.index -= 1;

            if value > 0 {
                return Some(index);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_increment_within_bounds() {
        let mut histo = Histogram::new(1);

        histo.increment(0, 10);

        assert_eq!(histo.get(0), 10);
    }

    #[test]
    fn test_increment_successive_within_bounds() {
        let mut histo = Histogram::new(1);

        histo.increment(0, 5);
        histo.increment(0, 5);

        assert_eq!(histo.get(0), 10);
    }

    #[test]
    #[should_panic]
    fn test_increment_out_of_bounds() {
        let mut histo = Histogram::new(1);

        histo.increment(2, 10);
    }

    #[test]
    fn test_most_fragmented_bin() {
        let mut histo = Histogram::new(2);

        histo.increment(0, 5);
        histo.increment(1, 7);

        assert_eq!(histo.most_fragmented_bin(), 1);
    }

    #[test]
    fn test_iter() {
        let mut histo = Histogram::new(3);

        histo.increment(0, 10);
        histo.increment(1, 20);
        histo.increment(2, 25);

        let mut iter = histo.iter();

        assert_eq!(iter.next().unwrap(), 2);
        assert_eq!(iter.next().unwrap(), 1);
        assert_eq!(iter.next().unwrap(), 0);
        assert!(iter.next().is_none());
    }
}
