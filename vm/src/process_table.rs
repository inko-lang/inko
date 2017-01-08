//! Table for storing PIDs and mapping them to processes.
//!
//! A ProcessTable can be used for reserving PIDs and mapping these to processes
//! (e.g. a Process/Arc<Process> structure).
//!
//! Basic usage is broken up in two steps:
//!
//! 1. Reserve a PID
//! 1. Store a process in the process table using said PID
//!
//! For example:
//!
//!     let table = ProcessTable::new();
//!
//!     if let Some(pid) = table.reserve() {
//!         table.map(pid, some_process);
//!     } else {
//!         panic!("No PIDs available!");
//!     }
//!
//! ## Recycling
//!
//! PIDs are recycled once a certain number of PIDs has been used. The exact
//! amount of PIDs that can be used before recycling depends on the system
//! architecture. In most cases PID recycling will only occur very rarely, if
//! ever at all.
//!
//! ## PID Availability
//!
//! It's possible (though very unlikely) for a ProcessTable to run out of
//! available PIDs. This can happen when many processes are added and kept
//! around. Callers should ensure they can handle such a scenario.

use std::usize;
use std::collections::HashMap;

/// The type of a PID.
pub type PID = usize;

/// The maximum PID value.
pub const MAX_PID: PID = usize::MAX;

pub struct ProcessTable<T: Clone> {
    /// The PID to use for the next process.
    next_pid: PID,

    /// When set to true, previously used PIDs may be recycled.
    recycle: bool,

    /// PIDs of existing processes, and their corresponding processes.
    ///
    /// An entry's value may be set to None, indicating that the PID has been
    /// reserved but a process has yet to be inserted.
    processes: HashMap<PID, Option<T>>,
}

impl<T: Clone> ProcessTable<T> {
    pub fn new() -> Self {
        ProcessTable {
            next_pid: 0,
            recycle: false,
            processes: HashMap::new(),
        }
    }

    /// Reserves a new PID.
    ///
    /// If no PID could be reserved a None value is returned.
    pub fn reserve(&mut self) -> Option<PID> {
        let available = self.processes.len() < MAX_PID;

        while available {
            let pid = self.next_pid();

            if self.recycle && self.processes.contains_key(&pid) {
                continue;
            }

            self.processes.insert(pid, None);

            return Some(pid);
        }

        None
    }

    /// Maps a process to the given PID.
    pub fn map(&mut self, pid: PID, process: T) {
        self.processes.insert(pid, Some(process));
    }

    /// Releases a PID.
    pub fn release(&mut self, pid: &PID) {
        self.processes.remove(pid);
    }

    /// Returns the process for a given PID.
    pub fn get(&self, pid: &PID) -> Option<T> {
        if let Some(slot) = self.processes.get(pid) {
            match slot {
                &Some(ref process) => Some(process.clone()),
                &None => None,
            }
        } else {
            None
        }
    }

    fn next_pid(&mut self) -> PID {
        let pid = self.next_pid;

        if pid == MAX_PID {
            self.next_pid = 0;
            self.recycle = true;
        } else {
            self.next_pid += 1;
        }

        pid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let table = ProcessTable::<()>::new();

        assert_eq!(table.next_pid, 0);
        assert_eq!(table.recycle, false);
        assert_eq!(table.processes.len(), 0);
    }

    #[test]
    fn test_reserve() {
        let mut table = ProcessTable::<()>::new();
        let pid = table.reserve();

        assert!(pid.is_some());
        assert_eq!(pid.unwrap(), 0);

        let pid2 = table.reserve();

        assert!(pid2.is_some());
        assert_eq!(pid2.unwrap(), 1);
    }

    #[test]
    fn test_reserve_with_recycle() {
        let mut table = ProcessTable::<()>::new();

        table.reserve();
        table.next_pid = 0;
        table.recycle = true;

        let pid2 = table.reserve();

        assert_eq!(pid2.unwrap(), 1);
    }

    #[test]
    fn test_map() {
        let mut table = ProcessTable::new();
        let pid = table.reserve().unwrap();

        table.map(pid, 10);

        assert_eq!(table.get(&pid).unwrap(), 10);
    }

    #[test]
    fn test_release() {
        let mut table = ProcessTable::new();
        let pid = table.reserve().unwrap();

        table.map(pid, 10);
        table.release(&pid);

        assert!(table.get(&pid).is_none());
    }

    #[test]
    fn test_get() {
        let mut table = ProcessTable::new();

        assert!(table.get(&0).is_none());

        let pid = table.reserve().unwrap();

        assert!(table.get(&pid).is_none());

        table.map(pid, 10);

        assert!(table.get(&pid).is_some());
        assert_eq!(table.get(&pid).unwrap(), 10);
    }
}
