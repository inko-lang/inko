//! Processes suspended with a timeout.
use crate::process::ProcessPointer;
use crate::state::State;
use std::cmp;
use std::cmp::max;
use std::collections::{BinaryHeap, HashMap};
use std::num::NonZeroU64;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// The percentage of timeouts (from 0.0 to 1.0) that can be expired before the
/// timeouts heap must be cleaned up.
const FRAGMENTATION_THRESHOLD: f64 = 0.1;

/// The shortest amount of time we'll sleep for when timeouts are present, in
/// milliseconds.
const MIN_SLEEP_TIME: u64 = 10;

/// A point in the future relative to the runtime's monotonic clock.
#[derive(Eq, PartialEq)]
pub(crate) struct Deadline {
    /// The time after which to resume, in nanoseconds since the runtime epoch.
    ///
    /// We use a `u64` here rather than an Instant for two reasons:
    ///
    /// 1. It only needs 8 bytes instead of 16
    /// 2. It makes some of the internal calculations easier due to the use of
    ///    our own epoch
    resume_after: u64,
}

impl Deadline {
    pub(crate) fn until(nanos: u64) -> Self {
        Deadline { resume_after: nanos }
    }

    pub(crate) fn duration(state: &State, duration: Duration) -> Self {
        let deadline =
            (Instant::now() - state.start_time + duration).as_nanos() as u64;

        Deadline::until(deadline)
    }

    pub(crate) fn remaining_time(&self, state: &State) -> Option<Duration> {
        (state.start_time + Duration::from_nanos(self.resume_after))
            .checked_duration_since(Instant::now())
    }
}

#[derive(Eq, PartialEq, Hash, Copy, Clone, Debug)]
pub(crate) struct Id(pub(crate) NonZeroU64);

/// A single timeout in the timeout heap.
struct Timeout {
    time: Deadline,
    id: Id,
}

impl Timeout {
    pub(crate) fn new(id: Id, timeout: Deadline) -> Self {
        Timeout { time: timeout, id }
    }
}

impl PartialOrd for Timeout {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timeout {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        // BinaryHeap pops values starting with the greatest value, but we want
        // values with the smallest timeouts. To achieve this, we reverse the
        // sorting order for this type.
        self.time.resume_after.cmp(&other.time.resume_after).reverse()
    }
}

impl PartialEq for Timeout {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.id == other.id
    }
}

impl Eq for Timeout {}

/// A collection of processes that are waiting with a timeout.
///
/// This structure uses a binary heap for two reasons:
///
/// 1. At the time of writing, no mature and maintained timer wheels exist for
///    Rust. The closest is tokio-timer, but this requires the use of tokio.
/// 2. Binary heaps allow for arbitrary precision timeouts, at the cost of
///    insertions being more expensive.
pub(crate) struct Timeouts {
    /// All timeouts (including expired entries to be removed), sorted from
    /// shortest to longest.
    entries: BinaryHeap<Timeout>,

    /// The timeouts that are active.
    ///
    /// If a process is rescheduled while a timeout is active, it's entry is to
    /// be removed from this map.
    ///
    /// The keys are the entry IDs, and the values the pointer to the process
    /// the ID belongs to.
    active: HashMap<Id, ProcessPointer>,

    /// The ID to assign to the next entry.
    ///
    /// While in theory this value can overflow, the chances of that happening
    /// are astronomically low: even if it takes 1 nanosecond to perform all the
    /// work necessary, it would still take 584 years for the counter to
    /// overflow.
    next_id: NonZeroU64,

    /// The amount of entries that have been expired before they ran out of
    /// time.
    expired: usize,
}

impl Timeouts {
    pub(crate) fn new() -> Self {
        Timeouts {
            entries: BinaryHeap::new(),
            next_id: NonZeroU64::new(1).unwrap(),
            active: HashMap::new(),
            expired: 0,
        }
    }

    pub(crate) fn insert(
        &mut self,
        process: ProcessPointer,
        deadline: Deadline,
    ) -> Id {
        let id = Id(self.next_id);

        self.next_id =
            self.next_id.checked_add(1).or(NonZeroU64::new(1)).unwrap();
        self.entries.push(Timeout::new(id, deadline));
        self.active.insert(id, process);
        id
    }

    pub(crate) fn expire(&mut self, id: Id) {
        self.active.remove(&id);
        self.expired += 1;
    }

    pub(crate) fn compact(&mut self) {
        let len = self.entries.len();

        if len == 0 {
            return;
        }

        let ratio = self.expired as f64 / len as f64;

        if ratio >= FRAGMENTATION_THRESHOLD {
            self.remove_expired();
            self.expired = 0;
        }
    }

    pub(crate) fn processes_to_reschedule(
        &mut self,
        state: &State,
        reschedule: &mut Vec<ProcessPointer>,
    ) -> Option<Duration> {
        let mut time_until_expiration = None;

        while let Some(entry) = self.entries.pop() {
            let Some(&proc) = self.active.get(&entry.id) else { continue };
            let mut proc_state = proc.state();

            if let Some(duration) = entry.time.remaining_time(state) {
                drop(proc_state);
                self.entries.push(entry);
                time_until_expiration = Some(duration);

                // If this timeout didn't expire yet, any following timeouts
                // also haven't expired.
                break;
            }

            self.active.remove(&entry.id);

            if proc_state.try_reschedule_after_timeout().are_acquired() {
                drop(proc_state);

                // Safety: we are holding on to the process run lock, so the
                // process pointer won't be invalidated.
                reschedule.push(proc);
            }
        }

        time_until_expiration
    }

    fn remove_expired(&mut self) {
        self.entries = self
            .entries
            .drain()
            .filter(|entry| self.active.contains_key(&entry.id))
            .collect();
    }
}

/// A TimeoutWorker is tasked with rescheduling processes when their timeouts
/// expire.
pub(crate) struct Worker {
    timeouts: Mutex<Timeouts>,
    cvar: Condvar,
}

impl Worker {
    pub(crate) fn new() -> Self {
        Worker { timeouts: Mutex::new(Timeouts::new()), cvar: Condvar::new() }
    }

    pub(crate) fn suspend(
        &self,
        process: ProcessPointer,
        deadline: Deadline,
    ) -> Id {
        let mut timeouts = self.timeouts.lock().unwrap();
        let id = timeouts.insert(process, deadline);

        self.cvar.notify_one();
        id
    }

    pub(crate) fn expire(&self, id: Id) {
        let mut timeouts = self.timeouts.lock().unwrap();

        timeouts.expire(id);
        self.cvar.notify_one();
    }

    pub(crate) fn terminate(&self) {
        let mut _timeouts = self.timeouts.lock().unwrap();

        self.cvar.notify_one();
    }

    pub(crate) fn run(&self, state: &State) {
        let mut expired = Vec::new();

        while state.scheduler.is_alive() {
            self.run_iteration(state, &mut expired);
        }
    }

    fn run_iteration(&self, state: &State, expired: &mut Vec<ProcessPointer>) {
        let mut timeouts = self.timeouts.lock().unwrap();

        timeouts.compact();

        let next_dur = timeouts.processes_to_reschedule(state, expired);

        state.scheduler.schedule_multiple(expired);

        // If at this point we're shutting down the scheduler, so should we shut
        // down ourselves.
        if !state.scheduler.is_alive() {
            return;
        }

        // In the event of a spurious wakeup we just move on to the next
        // iteration of the run loop.
        if let Some(time) = next_dur {
            let dur = max(Duration::from_millis(MIN_SLEEP_TIME), time);
            let _res = self.cvar.wait_timeout(timeouts, dur).unwrap();
        } else {
            let _lock = self.cvar.wait(timeouts).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{empty_process_type, new_process, setup};
    use std::mem::size_of;
    use std::thread::sleep;

    mod timeout {
        use super::*;

        #[test]
        fn test_type_size() {
            assert_eq!(size_of::<Deadline>(), 8);
        }

        #[test]
        fn test_remaining_time_with_remaining_time() {
            let state = setup();
            let timeout = Deadline::duration(&state, Duration::from_secs(10));
            let remaining = timeout.remaining_time(&state);

            assert!(remaining >= Some(Duration::from_secs(9)));
        }

        #[test]
        fn test_remaining_time_without_remaining_time() {
            let state = setup();
            let timeout = Deadline::duration(&state, Duration::from_nanos(0));
            let remaining = timeout.remaining_time(&state);

            sleep(Duration::from_millis(10));
            assert!(remaining.is_none());
        }
    }

    mod timeout_entry {
        use super::*;
        use std::cmp;

        #[test]
        fn test_partial_cmp() {
            let state = setup();
            let t1 = Timeout::new(
                Id(NonZeroU64::new(1).unwrap()),
                Deadline::duration(&state, Duration::from_secs(1)),
            );

            let t2 = Timeout::new(
                Id(NonZeroU64::new(2).unwrap()),
                Deadline::duration(&state, Duration::from_secs(5)),
            );

            assert_eq!(t1.partial_cmp(&t2), Some(cmp::Ordering::Greater));
            assert_eq!(t2.partial_cmp(&t1), Some(cmp::Ordering::Less));
        }

        #[test]
        fn test_cmp() {
            let state = setup();
            let t1 = Timeout::new(
                Id(NonZeroU64::new(1).unwrap()),
                Deadline::duration(&state, Duration::from_secs(1)),
            );

            let t2 = Timeout::new(
                Id(NonZeroU64::new(2).unwrap()),
                Deadline::duration(&state, Duration::from_secs(5)),
            );

            assert_eq!(t1.cmp(&t2), cmp::Ordering::Greater);
            assert_eq!(t2.cmp(&t1), cmp::Ordering::Less);
        }

        #[test]
        fn test_eq() {
            let state = setup();
            let t1 = Timeout::new(
                Id(NonZeroU64::new(1).unwrap()),
                Deadline::duration(&state, Duration::from_secs(1)),
            );

            let t2 = Timeout::new(
                Id(NonZeroU64::new(2).unwrap()),
                Deadline::duration(&state, Duration::from_secs(5)),
            );

            assert!(t1 == t1);
            assert!(t1 != t2);
        }
    }

    mod timeouts {
        use super::*;

        #[test]
        fn test_insert() {
            let state = setup();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer());
            let mut timeouts = Timeouts::new();
            let timeout = Deadline::duration(&state, Duration::from_secs(10));

            timeouts.insert(*process, timeout);
            assert_eq!(timeouts.entries.len(), 1);
        }

        #[test]
        fn test_remove_invalid_entries_with_valid_entries() {
            let state = setup();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer());
            let mut timeouts = Timeouts::new();
            let timeout = Deadline::duration(&state, Duration::from_secs(10));
            let id = timeouts.insert(*process, timeout);

            process.state().waiting_for_value(Some(id));

            timeouts.remove_expired();
            assert_eq!(timeouts.entries.len(), 1);
        }

        #[test]
        fn test_remove_invalid_entries_with_invalid_entries() {
            let state = setup();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer());
            let mut timeouts = Timeouts::new();
            let timeout = Deadline::duration(&state, Duration::from_secs(10));
            let id = timeouts.insert(*process, timeout);

            timeouts.active.remove(&id);
            timeouts.remove_expired();
            assert_eq!(timeouts.entries.len(), 0);
        }

        #[test]
        fn test_processes_to_reschedule_with_invalid_entries() {
            let state = setup();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer());
            let mut timeouts = Timeouts::new();
            let timeout = Deadline::duration(&state, Duration::from_secs(10));
            let id = timeouts.insert(*process, timeout);

            timeouts.active.remove(&id);

            let mut reschedule = Vec::new();
            let expiration =
                timeouts.processes_to_reschedule(&state, &mut reschedule);

            assert!(reschedule.is_empty());
            assert!(expiration.is_none());
        }

        #[test]
        fn test_processes_to_reschedule_with_remaining_time() {
            let state = setup();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer());
            let mut timeouts = Timeouts::new();
            let timeout = Deadline::duration(&state, Duration::from_secs(10));
            let id = timeouts.insert(*process, timeout);

            process.state().waiting_for_value(Some(id));

            let mut reschedule = Vec::new();
            let expiration =
                timeouts.processes_to_reschedule(&state, &mut reschedule);

            assert!(reschedule.is_empty());
            assert!(expiration.is_some());
            assert!(expiration.unwrap() <= Duration::from_secs(10));
        }

        #[test]
        fn test_processes_to_reschedule_with_entries_to_reschedule() {
            let state = setup();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer());
            let mut timeouts = Timeouts::new();
            let timeout = Deadline::duration(&state, Duration::from_secs(0));
            let id = timeouts.insert(*process, timeout);

            process.state().waiting_for_value(Some(id));

            let mut reschedule = Vec::new();
            let expiration =
                timeouts.processes_to_reschedule(&state, &mut reschedule);

            assert_eq!(reschedule.len(), 1);
            assert!(expiration.is_none());
        }

        #[test]
        fn test_compact() {
            let mut timeouts = Timeouts::new();
            let typ = empty_process_type();
            let p1 = new_process(typ.as_pointer());
            let p2 = new_process(typ.as_pointer());
            let a = timeouts.insert(*p1, Deadline::until(0));
            let b = timeouts.insert(*p1, Deadline::until(0));
            let c = timeouts.insert(*p1, Deadline::until(0));
            let d = timeouts.insert(*p2, Deadline::until(0));

            timeouts.expire(a);
            timeouts.expire(b);

            assert_eq!(timeouts.expired, 2);
            timeouts.compact();

            assert!(timeouts.active.contains_key(&c));
            assert!(timeouts.active.contains_key(&d));
            assert_eq!(timeouts.entries.len(), 2);
        }
    }

    mod worker {
        use super::*;
        use crate::test::{empty_process_type, new_process, setup};

        #[test]
        fn test_expire() {
            let state = setup();
            let worker = Worker::new();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer());
            let id = worker.suspend(
                *process,
                Deadline::duration(&state, Duration::from_secs(1)),
            );

            worker.expire(id);
            assert!(worker.timeouts.lock().unwrap().active.is_empty());
        }

        #[test]
        fn test_suspend() {
            let state = setup();
            let worker = Worker::new();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer());
            let id = worker.suspend(
                *process,
                Deadline::duration(&state, Duration::from_secs(1)),
            );

            assert_eq!(id, Id(NonZeroU64::new(1).unwrap()));
        }

        #[test]
        fn test_run_with_reschedule() {
            let state = setup();
            let typ = empty_process_type();
            let process = new_process(typ.as_pointer()).take_and_forget();
            let worker = Worker::new();
            let timeout = Deadline::duration(&state, Duration::from_secs(0));
            let mut reschedule = Vec::new();

            state.scheduler.terminate();
            process
                .state()
                .waiting_for_value(Some(worker.suspend(process, timeout)));
            worker.run_iteration(&state, &mut reschedule);

            assert_eq!(worker.timeouts.lock().unwrap().entries.len(), 0);
            assert_eq!(reschedule.len(), 0);
        }
    }
}
