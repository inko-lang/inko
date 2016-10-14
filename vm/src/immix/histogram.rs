//! Histograms for marked and available lines.
//!
//! A Histogram is used to track the distribution of marked and available lines
//! across Immix blocks. Each bin represents the number of holes with the values
//! representing the number of marked lines.

pub struct Histogram {
    values: Vec<usize>,
}

/// Iterator for traversing the most fragmented bins in a histogram.
pub struct HistogramIterator<'a> {
    histogram: &'a Histogram,
    index: isize,
}

impl Histogram {
    pub fn new() -> Self {
        Histogram { values: Vec::new() }
    }

    /// Increments a bin by the given value.
    pub fn increment(&mut self, index: usize, value: usize) {
        if index >= self.values.len() {
            self.values.resize(index + 1, 0);
        }

        if self.values.get(index).is_none() {
            self.values[index] = 0;
        }

        self.values[index] += value;
    }

    /// Returns the value for the given bin.
    pub fn get(&self, index: usize) -> Option<usize> {
        self.values.get(index).cloned()
    }

    /// Returns the most fragmented bin.
    pub fn most_fragmented_bin(&self) -> usize {
        for (bin, value) in self.values.iter().enumerate().rev() {
            if *value > 0 {
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
        self.values.clear();
        self.values.shrink_to_fit();
    }
}

impl<'a> Iterator for HistogramIterator<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.index >= 0 {
            let index = self.index as usize;
            let value_opt = self.histogram.get(index as usize);

            self.index -= 1;

            if let Some(value) = value_opt {
                if value > 0 {
                    return Some(index);
                }
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
        let mut histo = Histogram::new();

        histo.increment(0, 10);

        assert!(histo.get(0).is_some());
        assert_eq!(histo.get(0).unwrap(), 10);
    }

    #[test]
    fn test_increment_successive_within_bounds() {
        let mut histo = Histogram::new();

        histo.increment(0, 5);
        histo.increment(0, 5);

        assert_eq!(histo.get(0).unwrap(), 10);
    }

    #[test]
    fn test_increment_out_of_bounds() {
        let mut histo = Histogram::new();

        histo.increment(2, 10);
        histo.increment(4, 20);

        for index in 0..4 {
            assert!(histo.get(index).is_some());
        }

        assert_eq!(histo.get(0).unwrap(), 0);
        assert_eq!(histo.get(1).unwrap(), 0);
        assert_eq!(histo.get(2).unwrap(), 10);
        assert_eq!(histo.get(3).unwrap(), 0);
        assert_eq!(histo.get(4).unwrap(), 20);
    }

    #[test]
    fn test_most_fragmented_bin() {
        let mut histo = Histogram::new();

        histo.increment(10, 5);
        histo.increment(15, 7);

        assert_eq!(histo.most_fragmented_bin(), 15);
    }

    #[test]
    fn test_iter() {
        let mut histo = Histogram::new();

        histo.increment(0, 10);
        histo.increment(5, 20);
        histo.increment(10, 25);

        let mut iter = histo.iter();

        assert_eq!(iter.next().unwrap(), 10);
        assert_eq!(iter.next().unwrap(), 5);
        assert_eq!(iter.next().unwrap(), 0);
        assert!(iter.next().is_none());
    }
}
