//! Tracing and marking of live objects.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::broadcast::Broadcast;
use crate::gc::statistics::TraceStatistics;
use crate::object::ObjectStatus;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use crate::process::RcProcess;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use std::thread;

#[derive(Clone)]
struct TraceJob {
    /// The process that is traced.
    process: RcProcess,

    /// If objects need to be moved around.
    moving: bool,
}

/// The shared inner state of a pool.
struct PoolState {
    /// A global queue to steal jobs from.
    global_queue: Injector<ObjectPointerPointer>,

    /// The list of queues we can steal work from.
    stealers: Vec<Stealer<ObjectPointerPointer>>,

    /// The sending end up the results channel.
    result_sender: Sender<TraceStatistics>,

    /// The receiving end up the results channel.
    result_receiver: Receiver<TraceStatistics>,

    /// A channel for broadcasting jobs to tracers.
    broadcast: Broadcast<TraceJob>,
}

impl PoolState {
    pub fn new(stealers: Vec<Stealer<ObjectPointerPointer>>) -> Self {
        let (result_sender, result_receiver) = unbounded();

        PoolState {
            global_queue: Injector::new(),
            stealers,
            result_sender,
            result_receiver,
            broadcast: Broadcast::new(),
        }
    }
}

/// A pool of Tracers all tracing the same process.
pub struct Pool {
    /// The internal state available to all tracers.
    state: ArcWithoutWeak<PoolState>,
}

impl Pool {
    pub fn new(threads: usize) -> Pool {
        let mut workers = Vec::with_capacity(threads);
        let mut stealers = Vec::with_capacity(threads);

        for _ in 0..threads {
            let worker = Worker::new_fifo();
            let stealer = worker.stealer();

            workers.push(worker);
            stealers.push(stealer);
        }

        let state = ArcWithoutWeak::new(PoolState::new(stealers));

        for worker in workers.into_iter() {
            let state = state.clone();

            thread::spawn(move || {
                Tracer::new(worker, state).run();
            });
        }

        Pool { state }
    }

    pub fn schedule(&self, pointer: ObjectPointerPointer) {
        self.state.global_queue.push(pointer);
    }

    pub fn trace(&self, process: &RcProcess, moving: bool) -> TraceStatistics {
        let mut result = TraceStatistics::new();
        let mut pending = self.state.stealers.len();
        let trace_job = TraceJob {
            process: process.clone(),
            moving,
        };

        self.state.broadcast.send(pending, trace_job);

        while pending > 0 {
            if let Ok(received) = self.state.result_receiver.recv() {
                result += received;
            } else {
                break;
            }

            pending -= 1;
        }

        result
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.state.broadcast.shutdown();
    }
}

/// A single thread tracing through live objects.
///
/// A Tracer can trace objects with or without the need for moving objects, and
/// both approaches use a specialised method for this to ensure optimal
/// performance.
///
/// A Tracer does not check the age of an object to determine if it should be
/// traced or not. Any mature object that is reachable is always marked, as
/// young collections do not reset mature mark states and mature collections
/// would mark any live objects again.
///
/// A Tracer may steal work from other Tracers in a Tracer pool, balancing work
/// across CPU cores. Due to the inherent racy nature of work-stealing its
/// possible one or more tracers don't perform any work. This can happen if
/// other Tracers steal all work before our Tracer can even begin.
struct Tracer {
    /// The local queue of objects to trace.
    queue: Worker<ObjectPointerPointer>,

    /// The inner state of the tracer pool.
    state: ArcWithoutWeak<PoolState>,
}

impl Tracer {
    pub fn new(
        queue: Worker<ObjectPointerPointer>,
        state: ArcWithoutWeak<PoolState>,
    ) -> Self {
        Self { queue, state }
    }

    pub fn run(&self) {
        loop {
            if let Some(job) = self.state.broadcast.recv() {
                let result = if job.moving {
                    self.trace_with_moving(&job.process)
                } else {
                    self.trace_without_moving()
                };

                if self.state.result_sender.send(result).is_err() {
                    return;
                }
            } else {
                return;
            }
        }
    }

    /// Traces through all live objects, without moving any objects.
    fn trace_without_moving(&self) -> TraceStatistics {
        let mut stats = TraceStatistics::new();

        while let Some(pointer_pointer) = self.pop_job() {
            let pointer = pointer_pointer.get();

            if pointer.is_marked() {
                continue;
            }

            pointer.mark();

            stats.marked += 1;

            self.schedule_child_pointers(*pointer);
        }

        stats
    }

    /// Traces through all live objects, moving them if needed.
    fn trace_with_moving(&self, process: &RcProcess) -> TraceStatistics {
        let mut stats = TraceStatistics::new();

        while let Some(pointer_pointer) = self.pop_job() {
            let pointer = pointer_pointer.get_mut();

            if pointer.is_marked() {
                continue;
            }

            match pointer.status() {
                ObjectStatus::Resolve => pointer.resolve_forwarding_pointer(),
                ObjectStatus::Promote => {
                    self.promote_mature(process, pointer);

                    stats.promoted += 1;
                    stats.marked += 1;

                    pointer.mark();

                    // When promoting an object we already trace it, so we
                    // don't need to trace it again below.
                    continue;
                }
                ObjectStatus::Evacuate => {
                    self.evacuate(process, pointer);

                    stats.evacuated += 1;
                }
                ObjectStatus::PendingMove => {
                    self.queue.push(pointer_pointer.clone());
                    continue;
                }
                _ => {}
            }

            pointer.mark();
            stats.marked += 1;

            self.schedule_child_pointers(*pointer);
        }

        stats
    }

    fn schedule_child_pointers(&self, pointer: ObjectPointer) {
        pointer.get().each_pointer(|child| {
            self.queue.push(child);
        });
    }

    /// Traces a promoted object to see if it should be remembered in the
    /// remembered set.
    fn trace_promoted_object(
        &self,
        process: &RcProcess,
        promoted: ObjectPointer,
    ) {
        let mut remember = false;

        promoted.get().each_pointer(|child| {
            if !remember && child.get().is_young() {
                process.remember_object(promoted);

                remember = true;
            }

            self.queue.push(child);
        });
    }

    /// Promotes an object to the mature generation.
    ///
    /// The pointer to promote is updated to point to the new location.
    fn promote_mature(&self, process: &RcProcess, pointer: &mut ObjectPointer) {
        let local_data = process.local_data_mut();
        let old_obj = pointer.get_mut();
        let new_pointer = local_data.allocator.allocate_mature(old_obj.take());

        old_obj.forward_to(new_pointer);

        pointer.resolve_forwarding_pointer();

        self.trace_promoted_object(process, *pointer);
    }

    // Evacuates a pointer.
    //
    // The pointer to evacuate is updated to point to the new location.
    fn evacuate(&self, process: &RcProcess, pointer: &mut ObjectPointer) {
        // When evacuating an object we must ensure we evacuate the object into
        // the same bucket.
        let local_data = process.local_data_mut();
        let bucket = pointer.block_mut().bucket_mut().unwrap();
        let old_obj = pointer.get_mut();
        let new_obj = old_obj.take();

        let (_, new_pointer) =
            bucket.allocate(&local_data.allocator.global_allocator, new_obj);

        old_obj.forward_to(new_pointer);

        pointer.resolve_forwarding_pointer();
    }

    /// Returns the next available job to process.
    ///
    /// This method uses a more verbose approach to retrieving jobs instead of
    /// chaining values using and_then, or_else, etc (e.g. as shown on
    /// https://docs.rs/crossbeam/0.7.3/crossbeam/deque/index.html). This is
    /// done for two reasons:
    ///
    /// 1. We found the explicit approach to be more readable.
    /// 2. Measuring the performance of both approaches, we found the explicit
    ///    (current) approach to be up to 30% faster.
    fn pop_job(&self) -> Option<ObjectPointerPointer> {
        if let Some(job) = self.queue.pop() {
            return Some(job);
        }

        loop {
            match self.state.global_queue.steal_batch_and_pop(&self.queue) {
                Steal::Retry => {}
                Steal::Empty => break,
                Steal::Success(job) => return Some(job),
            };
        }

        // We don't steal in random order, as we found that stealing in-order
        // performs better.
        for stealer in self.state.stealers.iter() {
            loop {
                match stealer.steal_batch_and_pop(&self.queue) {
                    Steal::Retry => {}
                    Steal::Empty => break,
                    Steal::Success(job) => return Some(job),
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::Object;
    use crate::object_value;
    use crate::vm::test::setup;

    fn tracer() -> Tracer {
        let state = ArcWithoutWeak::new(PoolState::new(Vec::new()));

        Tracer::new(Worker::new_fifo(), state)
    }

    #[test]
    fn test_promote_mature() {
        let (_machine, _block, process) = setup();
        let tracer = tracer();
        let mut pointer =
            process.allocate_without_prototype(object_value::float(15.0));

        let old_address = pointer.raw.raw as usize;

        tracer.promote_mature(&process, &mut pointer);

        let new_address = pointer.raw.raw as usize;

        assert!(old_address != new_address);
        assert!(pointer.is_mature());
        assert_eq!(pointer.float_value().unwrap(), 15.0);
    }

    #[test]
    fn test_evacuate() {
        let (_machine, _block, process) = setup();
        let tracer = tracer();
        let mut pointer =
            process.allocate_without_prototype(object_value::float(15.0));

        let old_address = pointer.raw.raw as usize;

        tracer.evacuate(&process, &mut pointer);

        let new_address = pointer.raw.raw as usize;

        assert!(old_address != new_address);
        assert_eq!(pointer.float_value().unwrap(), 15.0);
    }

    #[test]
    fn test_trace_with_moving_with_marked_mature() {
        let (_machine, _block, process) = setup();
        let pool = Pool::new(1);
        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent.add_attribute(&process, young_child, young_child);
        young_parent.block_mut().set_fragmented();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.block_mut().set_fragmented();
        mature.mark();

        pool.schedule(young_parent.pointer());
        pool.schedule(mature.pointer());

        let stats = pool.trace(&process, true);

        assert_eq!(stats.marked, 2);
        assert_eq!(stats.evacuated, 2);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_with_moving_with_unmarked_mature() {
        let (_machine, _block, process) = setup();
        let pool = Pool::new(1);
        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent.add_attribute(&process, young_child, young_child);
        young_parent.block_mut().set_fragmented();

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.block_mut().set_fragmented();

        pool.schedule(young_parent.pointer());
        pool.schedule(mature.pointer());

        let stats = pool.trace(&process, true);

        assert_eq!(stats.marked, 3);
        assert_eq!(stats.evacuated, 3);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_without_moving_with_marked_mature() {
        let (_machine, _block, process) = setup();
        let pool = Pool::new(1);
        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent.add_attribute(&process, young_child, young_child);

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        mature.mark();

        pool.schedule(young_parent.pointer());
        pool.schedule(mature.pointer());

        let stats = pool.trace(&process, false);

        assert!(young_parent.is_marked());
        assert!(young_child.is_marked());

        assert_eq!(stats.marked, 2);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_without_moving_with_unmarked_mature() {
        let (_machine, _block, process) = setup();
        let pool = Pool::new(1);
        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent.add_attribute(&process, young_child, young_child);

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        pool.schedule(young_parent.pointer());
        pool.schedule(mature.pointer());

        let stats = pool.trace(&process, false);

        assert!(young_parent.is_marked());
        assert!(young_child.is_marked());
        assert!(mature.is_marked());

        assert_eq!(stats.marked, 3);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);
    }
}
