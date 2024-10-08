use std::cmp::{Ord, Ordering, PartialOrd};
use std::fmt;
use std::ops::RangeInclusive;

/// The source location of a symbol.
///
/// This type doesn't use Rust's range types in order to keep its size down to a
/// minimum.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line_start: u32,
    pub line_end: u32,
    pub column_start: u32,
    pub column_end: u32,
}

impl Location {
    pub fn new(
        lines: &RangeInclusive<u32>,
        columns: &RangeInclusive<u32>,
    ) -> Location {
        Location {
            line_start: *lines.start(),
            line_end: *lines.end(),
            column_start: *columns.start(),
            column_end: *columns.end(),
        }
    }

    pub fn start_end(start: &Location, end: &Location) -> Location {
        Location {
            line_start: start.line_start,
            line_end: end.line_end,
            column_start: start.column_start,
            column_end: end.column_end,
        }
    }

    pub fn is_trailing(&self, other: &Location) -> bool {
        self.line_start == other.line_start || self.line_start == other.line_end
    }
}

impl Default for Location {
    fn default() -> Location {
        Location { line_start: 1, line_end: 1, column_start: 1, column_end: 1 }
    }
}

impl fmt::Debug for Location {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "lines {}..{}, columns {}..{}",
            self.line_start, self.line_end, self.column_start, self.column_end
        )
    }
}

impl PartialOrd for Location {
    fn partial_cmp(&self, other: &Location) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Location {
    fn cmp(&self, other: &Location) -> Ordering {
        let ord = self.line_start.cmp(&other.line_start);

        if ord == Ordering::Equal {
            return self.column_start.cmp(&other.column_start);
        }

        ord
    }
}
