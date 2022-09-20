//! Mapping of instruction offsets to their source locations.
use crate::mem::Pointer;

/// A single entry in a location table.
pub(crate) struct Entry {
    /// The instruction index.
    index: u32,

    /// The source line number.
    ///
    /// We don't use line number offsets, as line numbers are not monotonically
    /// increasing due to code inlining. For example, entry A may refer to line
    /// 4 in file A, while entry B may refer to line 2 in file D.
    ///
    /// This means files are limited to (2^16)-1 lines. This should be fine in
    /// practise, as files with that many lines are never a good idea.
    line: u16,

    /// The file (as a permanent string) the instruction originates from.
    file: Pointer,

    /// The name of the method (as a permanent string) the instruction
    /// originates from.
    name: Pointer,
}

/// A location resolved using a location table.
#[derive(Eq, PartialEq, Debug)]
pub(crate) struct Location {
    pub(crate) name: Pointer,
    pub(crate) file: Pointer,
    pub(crate) line: Pointer,
}

/// A table that maps instruction offsets to their source locations.
///
/// This table is used to obtain stack traces. The setup here is based on the
/// line number tables found in Python.
///
/// File paths and scope names are stored separately from entries. This ensures
/// entries take up as little space as possible.
pub(crate) struct LocationTable {
    entries: Vec<Entry>,
}

impl LocationTable {
    pub(crate) fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub(crate) fn add_entry(
        &mut self,
        index: u32,
        line: u16,
        file: Pointer,
        name: Pointer,
    ) {
        self.entries.push(Entry { index, line, file, name });
    }

    pub(crate) fn get(&self, index: u32) -> Option<Location> {
        for entry in self.entries.iter() {
            if entry.index == index {
                let name = entry.name;
                let file = entry.file;
                let line = Pointer::int(entry.line as i64);

                return Some(Location { name, file, line });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<Entry>(), 24);
    }

    #[test]
    fn test_location_table_get() {
        let mut table = LocationTable::new();
        let file = Pointer::int(1);
        let name = Pointer::int(2);

        table.add_entry(2, 1, file, name);
        table.add_entry(1, 2, file, name);
        table.add_entry(1, 3, file, name);

        assert!(table.get(0).is_none());
        assert_eq!(
            table.get(1),
            Some(Location { name, file, line: Pointer::int(2) })
        );
        assert_eq!(
            table.get(2),
            Some(Location { name, file, line: Pointer::int(1) })
        );
        assert!(table.get(3).is_none());
    }
}
