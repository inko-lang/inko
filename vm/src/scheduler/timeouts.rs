//! Processes suspended with a timeout.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::process::RcProcess;
use std::cmp;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

/// A process that should be resumed after a certain point in time.
pub struct Timeout {
    /// The time after which the timeout expires.
    resume_after: Instant,
}

impl Timeout {
    pub fn new(suspend_for: Duration) -> Self {
        Timeout {
            resume_after: Instant::now() + suspend_for,
        }
    }

    pub fn with_rc(suspend_for: Duration) -> ArcWithoutWeak<Self> {
        ArcWithoutWeak::new(Self::new(suspend_for))
    }

    pub fn remaining_time(&self) -> Option<Duration> {
        let now = Instant::now();

        if now >= self.resume_after {
            None
        } else {
            Some(self.resume_after - now)
        }
    }
}

/// A Timeout and a Process to store in the timeout heap.
///
/// Since the Timeout is also stored in a process we can't also store a Process
/// in a Timeout, as this would result in cyclic references. To work around
/// this, we store the two values in this separate TimeoutEntry structure.
struct TimeoutEntry {
    timeout: ArcWithoutWeak<Timeout>,
    process: RcProcess,
}

impl TimeoutEntry {
    pub fn new(process: RcProcess, timeout: ArcWithoutWeak<Timeout>) -> Self {
        TimeoutEntry { process, timeout }
    }

    fn is_valid(&self) -> bool {
        self.process.is_suspended_with_timeout(&self.timeout)
    }

    fn acquire_rescheduling_rights(&self) -> bool {
        self.process.acquire_rescheduling_rights().are_acquired()
    }
}

impl PartialOrd for TimeoutEntry {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimeoutEntry {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        // BinaryHeap pops values starting with the greatest value, but we want
        // values with the smallest timeouts. To achieve this, we reverse the
        // sorting order for this type.
        self.timeout
            .resume_after
            .cmp(&other.timeout.resume_after)
            .reverse()
    }
}

impl PartialEq for TimeoutEntry {
    fn eq(&self, other: &Self) -> bool {
        self.timeout.resume_after == other.timeout.resume_after
            && self.process == other.process
    }
}

impl Eq for TimeoutEntry {}

/// A collection of processes that are waiting with a timeout.
///
/// This structure uses a binary heap for two reasons:
///
/// 1. At the time of writing, no mature and maintained timer wheels exist for
///    Rust. The closest is tokio-timer, but this requires the use of tokio.
/// 2. Binary heaps allow for arbitrary precision timeouts, at the cost of
///    insertions being more expensive.
///
/// All timeouts are also stored in a hash map, making it cheaper to invalidate
/// timeouts at the cost of potentially keeping invalidated entries around in
/// the heap for a while.
pub struct Timeouts {
    /// The timeouts of all processes, sorted from shortest to longest.
    timeouts: BinaryHeap<TimeoutEntry>,
}

#[cfg_attr(feature = "cargo-clippy", allow(len_without_is_empty))]
impl Timeouts {
    pub fn new() -> Self {
        Timeouts {
            timeouts: BinaryHeap::new(),
        }
    }

    pub fn insert(
        &mut self,
        process: RcProcess,
        timeout: ArcWithoutWeak<Timeout>,
    ) {
        self.timeouts.push(TimeoutEntry::new(process, timeout));
    }

    pub fn len(&self) -> usize {
        self.timeouts.len()
    }

    pub fn remove_invalid_entries(&mut self) -> usize {
        let mut removed = 0;
        let new_heap = self
            .timeouts
            .drain()
            .filter(|entry| {
                if entry.is_valid() {
                    true
                } else {
                    removed += 1;
                    false
                }
            })
            .collect();

        self.timeouts = new_heap;

        removed
    }

    pub fn processes_to_reschedule(
        &mut self,
    ) -> (Vec<RcProcess>, Option<Duration>) {
        let mut reschedule = Vec::new();
        let mut time_until_expiration = None;

        while let Some(entry) = self.timeouts.pop() {
            if !entry.is_valid() {
                continue;
            }

            if let Some(duration) = entry.timeout.remaining_time() {
                self.timeouts.push(entry);

                time_until_expiration = Some(duration);

                // If this timeout didn't expire yet, any following timeouts
                // also haven't expired.
                break;
            }

            if entry.acquire_rescheduling_rights() {
                reschedule.push(entry.process);
            }
        }

        (reschedule, time_until_expiration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod timeout {
        use super::*;

        #[test]
        fn test_new() {
            let timeout = Timeout::new(Duration::from_secs(10));

            // Due to the above code taking a tiny bit of time to run we can't
            // assert that the "resume_after" field is _exactly_ 10 seconds from
            // now.
            let after = Instant::now() + Duration::from_secs(9);

            assert!(timeout.resume_after >= after);
        }

        #[test]
        fn test_with_rc() {
            let timeout = Timeout::with_rc(Duration::from_secs(10));
            let after = Instant::now() + Duration::from_secs(9);

            assert!(timeout.resume_after >= after);
        }

        #[test]
        fn test_remaining_time_with_remaining_time() {
            let timeout = Timeout::new(Duration::from_secs(10));
            let remaining = timeout.remaining_time();

            assert!(remaining.is_some());
            assert!(remaining.unwrap() >= Duration::from_secs(9));
        }

        #[test]
        fn test_remaining_time_without_remaining_time() {
            let timeout = Timeout::new(Duration::from_secs(0));

            assert!(timeout.remaining_time().is_none());
        }
    }

    mod timeout_entry {
        use super::*;
        use crate::vm::test::setup;
        use std::cmp;

        #[test]
        fn test_valid() {
            let (_machine, _block, process) = setup();
            let timeout = Timeout::with_rc(Duration::from_secs(1));
            let entry = TimeoutEntry::new(process.clone(), timeout.clone());

            assert_eq!(entry.is_valid(), false);

            process.suspend_with_timeout(timeout);

            assert!(entry.is_valid());
        }

        #[test]
        fn test_acquire_rescheduling_rights() {
            let (_machine, _block, process) = setup();
            let timeout = Timeout::with_rc(Duration::from_secs(1));
            let entry = TimeoutEntry::new(process.clone(), timeout.clone());

            process.suspend_with_timeout(timeout);

            assert_eq!(entry.acquire_rescheduling_rights(), true);
            assert_eq!(entry.acquire_rescheduling_rights(), false);
        }

        #[test]
        fn test_partial_cmp() {
            let (_machine, _block, process) = setup();
            let entry1 = TimeoutEntry::new(
                process.clone(),
                Timeout::with_rc(Duration::from_secs(1)),
            );

            let entry2 = TimeoutEntry::new(
                process.clone(),
                Timeout::with_rc(Duration::from_secs(5)),
            );

            assert_eq!(
                entry1.partial_cmp(&entry2),
                Some(cmp::Ordering::Greater)
            );

            assert_eq!(entry2.partial_cmp(&entry1), Some(cmp::Ordering::Less));
        }

        #[test]
        fn test_cmp() {
            let (_machine, _block, process) = setup();
            let entry1 = TimeoutEntry::new(
                process.clone(),
                Timeout::with_rc(Duration::from_secs(1)),
            );

            let entry2 = TimeoutEntry::new(
                process.clone(),
                Timeout::with_rc(Duration::from_secs(5)),
            );

            assert_eq!(entry1.cmp(&entry2), cmp::Ordering::Greater);
            assert_eq!(entry2.cmp(&entry1), cmp::Ordering::Less);
        }

        #[test]
        fn test_eq() {
            let (_machine, _block, process) = setup();
            let entry1 = TimeoutEntry::new(
                process.clone(),
                Timeout::with_rc(Duration::from_secs(1)),
            );

            let entry2 = TimeoutEntry::new(
                process.clone(),
                Timeout::with_rc(Duration::from_secs(5)),
            );

            assert!(entry1 == entry1);
            assert!(entry1 != entry2);
        }
    }

    mod timeouts {
        use super::*;
        use crate::vm::test::setup;

        #[test]
        fn test_insert() {
            let (_machine, _block, process) = setup();
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::with_rc(Duration::from_secs(10));

            timeouts.insert(process, timeout);

            assert_eq!(timeouts.timeouts.len(), 1);
        }

        #[test]
        fn test_len() {
            let (_machine, _block, process) = setup();
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::with_rc(Duration::from_secs(10));

            timeouts.insert(process, timeout);

            assert_eq!(timeouts.len(), 1);
        }

        #[test]
        fn test_remove_invalid_entries_with_valid_entries() {
            let (_machine, _block, process) = setup();
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::with_rc(Duration::from_secs(10));

            process.suspend_with_timeout(timeout.clone());
            timeouts.insert(process, timeout);

            assert_eq!(timeouts.remove_invalid_entries(), 0);
            assert_eq!(timeouts.len(), 1);
        }

        #[test]
        fn test_remove_invalid_entries_with_invalid_entries() {
            let (_machine, _block, process) = setup();
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::with_rc(Duration::from_secs(10));

            timeouts.insert(process, timeout);

            assert_eq!(timeouts.remove_invalid_entries(), 1);
            assert_eq!(timeouts.len(), 0);
        }

        #[test]
        fn test_processes_to_reschedule_with_invalid_entries() {
            let (_machine, _block, process) = setup();
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::with_rc(Duration::from_secs(10));

            timeouts.insert(process, timeout);

            let (reschedule, expiration) = timeouts.processes_to_reschedule();

            assert!(reschedule.is_empty());
            assert!(expiration.is_none());
        }

        #[test]
        fn test_processes_to_reschedule_with_remaining_time() {
            let (_machine, _block, process) = setup();
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::with_rc(Duration::from_secs(10));

            process.suspend_with_timeout(timeout.clone());
            timeouts.insert(process.clone(), timeout);

            let (reschedule, expiration) = timeouts.processes_to_reschedule();

            assert!(reschedule.is_empty());
            assert!(expiration.is_some());
            assert!(expiration.unwrap() <= Duration::from_secs(10));
        }

        #[test]
        fn test_processes_to_reschedule_with_entries_to_reschedule() {
            let (_machine, _block, process) = setup();
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::with_rc(Duration::from_secs(0));

            process.suspend_with_timeout(timeout.clone());
            timeouts.insert(process.clone(), timeout);

            let (reschedule, expiration) = timeouts.processes_to_reschedule();

            assert!(reschedule == vec![process]);
            assert!(expiration.is_none());
        }
    }
}
