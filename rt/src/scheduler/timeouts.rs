//! Processes suspended with a timeout.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::process::{Process, ProcessPointer};
use crate::state::State;
use std::cmp;
use std::collections::BinaryHeap;
use std::ops::Drop;
use std::time::{Duration, Instant};

/// An process that should be resumed after a certain point in time.
pub(crate) struct Timeout {
    /// The time after which to resume, in nanoseconds since the runtime epoch.
    ///
    /// We use a `u64` here rather than an Instant for two reasons:
    ///
    /// 1. It only needs 8 bytes instead of 16
    /// 2. It makes some of the internal calculations easier due to the use of
    ///    our own epoch
    resume_after: u64,
}

impl Timeout {
    pub(crate) fn until(nanos: u64) -> ArcWithoutWeak<Self> {
        ArcWithoutWeak::new(Timeout { resume_after: nanos })
    }

    pub(crate) fn duration(
        state: &State,
        duration: Duration,
    ) -> ArcWithoutWeak<Self> {
        let deadline =
            (Instant::now() - state.start_time + duration).as_nanos() as u64;

        Timeout::until(deadline)
    }

    pub(crate) fn remaining_time(&self, state: &State) -> Option<Duration> {
        (state.start_time + Duration::from_nanos(self.resume_after))
            .checked_duration_since(Instant::now())
    }
}

/// A Timeout and a Process to store in the timeout heap.
///
/// Since the Timeout is also stored in an process we can't also store an
/// process in a Timeout, as this would result in cyclic references. To work
/// around this, we store the two values in this separate TimeoutEntry
/// structure.
struct TimeoutEntry {
    timeout: ArcWithoutWeak<Timeout>,
    process: ProcessPointer,
}

impl TimeoutEntry {
    pub(crate) fn new(
        process: ProcessPointer,
        timeout: ArcWithoutWeak<Timeout>,
    ) -> Self {
        TimeoutEntry { timeout, process }
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
        self.timeout.resume_after.cmp(&other.timeout.resume_after).reverse()
    }
}

impl PartialEq for TimeoutEntry {
    fn eq(&self, other: &Self) -> bool {
        self.timeout.resume_after == other.timeout.resume_after
            && self.process.identifier() == other.process.identifier()
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
pub(crate) struct Timeouts {
    /// The timeouts of all processes, sorted from shortest to longest.
    timeouts: BinaryHeap<TimeoutEntry>,
}

#[cfg_attr(feature = "cargo-clippy", allow(clippy::len_without_is_empty))]
impl Timeouts {
    pub(crate) fn new() -> Self {
        Timeouts { timeouts: BinaryHeap::new() }
    }

    pub(crate) fn insert(
        &mut self,
        process: ProcessPointer,
        timeout: ArcWithoutWeak<Timeout>,
    ) {
        self.timeouts.push(TimeoutEntry::new(process, timeout));
    }

    pub(crate) fn len(&self) -> usize {
        self.timeouts.len()
    }

    pub(crate) fn remove_invalid_entries(&mut self) -> usize {
        let mut removed = 0;
        let new_heap = self
            .timeouts
            .drain()
            .filter(|entry| {
                if entry.process.state().has_same_timeout(&entry.timeout) {
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

    pub(crate) fn processes_to_reschedule(
        &mut self,
        state: &State,
    ) -> (Vec<ProcessPointer>, Option<Duration>) {
        let mut reschedule = Vec::new();
        let mut time_until_expiration = None;

        while let Some(entry) = self.timeouts.pop() {
            let mut proc_state = entry.process.state();

            if !proc_state.has_same_timeout(&entry.timeout) {
                continue;
            }

            if let Some(duration) = entry.timeout.remaining_time(state) {
                drop(proc_state);
                self.timeouts.push(entry);

                time_until_expiration = Some(duration);

                // If this timeout didn't expire yet, any following timeouts
                // also haven't expired.
                break;
            }

            if proc_state.try_reschedule_after_timeout().are_acquired() {
                drop(proc_state);
                reschedule.push(entry.process);
            }
        }

        (reschedule, time_until_expiration)
    }
}

impl Drop for Timeouts {
    fn drop(&mut self) {
        for entry in &self.timeouts {
            if entry.process.state().has_same_timeout(&entry.timeout) {
                // We may encounter outdated timeouts. In this case the process
                // may have been rescheduled and/or already dropped.
                Process::drop_and_deallocate(entry.process);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::Stack;
    use crate::test::{empty_process_class, new_process, setup};
    use std::mem::size_of;
    use std::thread::sleep;

    mod timeout {
        use super::*;

        #[test]
        fn test_type_size() {
            assert_eq!(size_of::<Timeout>(), 8);
        }

        #[test]
        fn test_remaining_time_with_remaining_time() {
            let state = setup();
            let timeout = Timeout::duration(&state, Duration::from_secs(10));
            let remaining = timeout.remaining_time(&state);

            assert!(remaining >= Some(Duration::from_secs(9)));
        }

        #[test]
        fn test_remaining_time_without_remaining_time() {
            let state = setup();
            let timeout = Timeout::duration(&state, Duration::from_nanos(0));
            let remaining = timeout.remaining_time(&state);

            sleep(Duration::from_millis(10));
            assert!(remaining.is_none());
        }
    }

    mod timeout_entry {
        use super::*;
        use crate::test::{empty_process_class, new_process};
        use std::cmp;

        #[test]
        fn test_partial_cmp() {
            let state = setup();
            let class = empty_process_class("A");
            let process = new_process(*class);
            let entry1 = TimeoutEntry::new(
                *process,
                Timeout::duration(&state, Duration::from_secs(1)),
            );

            let entry2 = TimeoutEntry::new(
                *process,
                Timeout::duration(&state, Duration::from_secs(5)),
            );

            assert_eq!(
                entry1.partial_cmp(&entry2),
                Some(cmp::Ordering::Greater)
            );

            assert_eq!(entry2.partial_cmp(&entry1), Some(cmp::Ordering::Less));
        }

        #[test]
        fn test_cmp() {
            let state = setup();
            let class = empty_process_class("A");
            let process = new_process(*class);
            let entry1 = TimeoutEntry::new(
                *process,
                Timeout::duration(&state, Duration::from_secs(1)),
            );

            let entry2 = TimeoutEntry::new(
                *process,
                Timeout::duration(&state, Duration::from_secs(5)),
            );

            assert_eq!(entry1.cmp(&entry2), cmp::Ordering::Greater);
            assert_eq!(entry2.cmp(&entry1), cmp::Ordering::Less);
        }

        #[test]
        fn test_eq() {
            let state = setup();
            let class = empty_process_class("A");
            let process = new_process(*class);
            let entry1 = TimeoutEntry::new(
                *process,
                Timeout::duration(&state, Duration::from_secs(1)),
            );

            let entry2 = TimeoutEntry::new(
                *process,
                Timeout::duration(&state, Duration::from_secs(5)),
            );

            assert!(entry1 == entry1);
            assert!(entry1 != entry2);
        }
    }

    mod timeouts {
        use super::*;

        #[test]
        fn test_insert() {
            let state = setup();
            let class = empty_process_class("A");
            let process = new_process(*class);
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::duration(&state, Duration::from_secs(10));

            timeouts.insert(*process, timeout);

            assert_eq!(timeouts.timeouts.len(), 1);
        }

        #[test]
        fn test_len() {
            let state = setup();
            let class = empty_process_class("A");
            let process = new_process(*class);
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::duration(&state, Duration::from_secs(10));

            timeouts.insert(*process, timeout);

            assert_eq!(timeouts.len(), 1);
        }

        #[test]
        fn test_remove_invalid_entries_with_valid_entries() {
            let state = setup();
            let class = empty_process_class("A");
            let process = Process::alloc(*class, Stack::new(1024));
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::duration(&state, Duration::from_secs(10));

            process.state().waiting_for_channel(Some(timeout.clone()));
            timeouts.insert(process, timeout);

            assert_eq!(timeouts.remove_invalid_entries(), 0);
            assert_eq!(timeouts.len(), 1);
        }

        #[test]
        fn test_remove_invalid_entries_with_invalid_entries() {
            let state = setup();
            let class = empty_process_class("A");
            let process = new_process(*class);
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::duration(&state, Duration::from_secs(10));

            timeouts.insert(*process, timeout);

            assert_eq!(timeouts.remove_invalid_entries(), 1);
            assert_eq!(timeouts.len(), 0);
        }

        #[test]
        fn test_processes_to_reschedule_with_invalid_entries() {
            let state = setup();
            let class = empty_process_class("A");
            let process = new_process(*class);
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::duration(&state, Duration::from_secs(10));

            timeouts.insert(*process, timeout);

            let (reschedule, expiration) =
                timeouts.processes_to_reschedule(&state);

            assert!(reschedule.is_empty());
            assert!(expiration.is_none());
        }

        #[test]
        fn test_processes_to_reschedule_with_remaining_time() {
            let state = setup();
            let class = empty_process_class("A");
            let process = Process::alloc(*class, Stack::new(1024));
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::duration(&state, Duration::from_secs(10));

            process.state().waiting_for_channel(Some(timeout.clone()));
            timeouts.insert(process, timeout);

            let (reschedule, expiration) =
                timeouts.processes_to_reschedule(&state);

            assert!(reschedule.is_empty());
            assert!(expiration.is_some());
            assert!(expiration.unwrap() <= Duration::from_secs(10));
        }

        #[test]
        fn test_processes_to_reschedule_with_entries_to_reschedule() {
            let state = setup();
            let class = empty_process_class("A");
            let process = new_process(*class);
            let mut timeouts = Timeouts::new();
            let timeout = Timeout::duration(&state, Duration::from_secs(0));

            process.state().waiting_for_channel(Some(timeout.clone()));
            timeouts.insert(*process, timeout);

            let (reschedule, expiration) =
                timeouts.processes_to_reschedule(&state);

            assert_eq!(reschedule.len(), 1);
            assert!(expiration.is_none());
        }
    }
}
