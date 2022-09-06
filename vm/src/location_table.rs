//! Mapping of instruction offsets to their source locations.
use crate::mem::Pointer;

/// A single entry in a location table.
pub(crate) struct Entry {
    /// The instruction offset.
    ///
    /// Offsets are relative to the previous offset, allowing for more than
    /// (2^16)-1 instructions per method.
    offset: u16,

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
        offset: u16,
        line: u16,
        file: Pointer,
        name: Pointer,
    ) {
        self.entries.push(Entry { offset, line, file, name });
    }

    pub(crate) fn get(&self, offset: usize) -> Option<Location> {
        let mut current = 0_usize;

        for entry in self.entries.iter() {
            current += entry.offset as usize;

            if current >= offset {
                // If we panic in these two lines that's fine, because we can't
                // do anything useful with busted bytecode anyway.
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

        assert_eq!(
            table.get(0),
            Some(Location { name, file, line: Pointer::int(1) })
        );

        assert_eq!(
            table.get(1),
            Some(Location { name, file, line: Pointer::int(1) })
        );

        assert_eq!(
            table.get(2),
            Some(Location { name, file, line: Pointer::int(1) })
        );

        assert_eq!(
            table.get(3),
            Some(Location { name, file, line: Pointer::int(2) })
        );

        assert_eq!(
            table.get(4),
            Some(Location { name, file, line: Pointer::int(3) })
        );

        assert_eq!(table.get(5), None);
        assert_eq!(table.get(10), None);
    }
}
