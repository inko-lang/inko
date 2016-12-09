//! Statistics produced by tracing object graphs.

use std::ops::Add;

pub struct TraceResult {
    /// The number of marked objects.
    pub marked: usize,

    /// The number of objects that were evacuated.
    pub evacuated: usize,

    /// The number of objects that were promoted to the mature generation.
    pub promoted: usize,
}

impl TraceResult {
    pub fn new() -> Self {
        Self::with(0, 0, 0)
    }

    pub fn with(marked: usize, evacuated: usize, promoted: usize) -> Self {
        TraceResult {
            marked: marked,
            evacuated: evacuated,
            promoted: promoted,
        }
    }
}

impl Add for TraceResult {
    type Output = TraceResult;

    fn add(self, other: Self::Output) -> Self::Output {
        TraceResult::with(self.marked + other.marked,
                          self.evacuated + other.evacuated,
                          self.promoted + other.promoted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let result = TraceResult::new();

        assert_eq!(result.marked, 0);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_add() {
        let a = TraceResult::with(5, 10, 15);
        let b = TraceResult::with(1, 2, 3);
        let c = a + b;

        assert_eq!(c.marked, 6);
        assert_eq!(c.evacuated, 12);
        assert_eq!(c.promoted, 18);
    }
}
