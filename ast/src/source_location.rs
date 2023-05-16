use std::cmp::{Ord, Ordering, PartialOrd};
use std::fmt;
use std::ops::RangeInclusive;

// The location of a single Inko expression.
#[derive(PartialEq, Eq, Clone)]
pub struct SourceLocation {
    /// The first and last line of the expression.
    pub line_range: RangeInclusive<usize>,

    /// The first and last column of the expression.
    pub column_range: RangeInclusive<usize>,
}

impl SourceLocation {
    pub fn new(
        line_range: RangeInclusive<usize>,
        column_range: RangeInclusive<usize>,
    ) -> Self {
        Self { line_range, column_range }
    }

    pub fn start_end(start: &Self, end: &Self) -> Self {
        Self {
            line_range: (*start.line_range.start())..=(*end.line_range.end()),
            column_range: (*start.column_range.start())
                ..=(*end.column_range.end()),
        }
    }

    pub fn line_column(&self) -> (usize, usize) {
        (*self.line_range.start(), *self.column_range.start())
    }
}

impl fmt::Debug for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "lines {}..{}, columns {}..{}",
            self.line_range.start(),
            self.line_range.end(),
            self.column_range.start(),
            self.column_range.end()
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
        let ord = self.line_range.start().cmp(other.line_range.start());

        if ord == Ordering::Equal {
            return self.column_range.start().cmp(other.column_range.start());
        }

        ord
    }
}
