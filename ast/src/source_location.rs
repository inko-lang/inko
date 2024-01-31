use std::cmp::{Ord, Ordering, PartialOrd};
use std::fmt;
use std::ops::RangeInclusive;

// The location of a single Inko expression.
#[derive(PartialEq, Eq, Clone)]
pub struct SourceLocation {
    /// The first and last line of the expression.
    pub lines: RangeInclusive<usize>,

    /// The first and last column of the expression.
    pub columns: RangeInclusive<usize>,
}

impl SourceLocation {
    pub fn new(
        line_range: RangeInclusive<usize>,
        column_range: RangeInclusive<usize>,
    ) -> Self {
        Self { lines: line_range, columns: column_range }
    }

    pub fn start_end(start: &Self, end: &Self) -> Self {
        Self {
            lines: (*start.lines.start())..=(*end.lines.end()),
            columns: (*start.columns.start())..=(*end.columns.end()),
        }
    }

    pub fn line_column(&self) -> (usize, usize) {
        (*self.lines.start(), *self.columns.start())
    }
}

impl fmt::Debug for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "lines {}..{}, columns {}..{}",
            self.lines.start(),
            self.lines.end(),
            self.columns.start(),
            self.columns.end()
        )
    }
}

impl PartialOrd for SourceLocation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SourceLocation {
    fn cmp(&self, other: &Self) -> Ordering {
        let ord = self.lines.start().cmp(other.lines.start());

        if ord == Ordering::Equal {
            return self.columns.start().cmp(other.columns.start());
        }

        ord
    }
}
