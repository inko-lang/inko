//! Immix histograms used for garbage collection.
use crate::immix::block::{LINES_PER_BLOCK, MAX_HOLES};
use crate::immix::histogram::Histogram;

/// A collection of histograms that Immix will use for determining when to move
/// objects.
pub struct Histograms {
    // The available space histogram for the blocks of this allocator.
    pub available: Histogram,

    /// The mark histogram for the blocks of this allocator.
    pub marked: Histogram,
}

unsafe impl Sync for Histograms {}

impl Histograms {
    pub fn new() -> Self {
        Self {
            available: Histogram::new(MAX_HOLES + 1),
            marked: Histogram::new(LINES_PER_BLOCK + 1),
        }
    }

    pub fn reset(&mut self) {
        self.available.reset();
        self.marked.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reset() {
        let mut histos = Histograms::new();

        histos.available.increment(0, 1);
        histos.reset();

        assert_eq!(histos.available.get(0), 0);
    }
}
