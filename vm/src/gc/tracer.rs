//! Tracing and marking of live objects.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::gc::statistics::TraceStatistics;
use crate::object::ObjectStatus;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use crate::process::RcProcess;
use crossbeam_deque::{Injector, Steal, Stealer, Worker};

/// A pool of Tracers all tracing the same process.
pub struct Pool {
    /// The process of which objects are being traced.
    process: RcProcess,

    /// A global queue to steal jobs from.
    global_queue: Injector<ObjectPointerPointer>,

    /// The list of queues we can steal work from.
    stealers: Vec<Stealer<ObjectPointerPointer>>,
}

impl Pool {
    pub fn new(
        process: RcProcess,
        threads: usize,
    ) -> (ArcWithoutWeak<Pool>, Vec<Tracer>) {
        let mut workers = Vec::new();
        let mut stealers = Vec::new();

        for _ in 0..threads {
            let worker = Worker::new_fifo();
            let stealer = worker.stealer();

            workers.push(worker);
            stealers.push(stealer);
        }

        let state = ArcWithoutWeak::new(Self {
            process,
            global_queue: Injector::new(),
            stealers,
        });

        let tracers = workers
            .into_iter()
            .map(|worker| Tracer::new(worker, state.clone()))
            .collect();

        (state, tracers)
    }

    pub fn schedule(&self, pointer: ObjectPointerPointer) {
        self.global_queue.push(pointer);
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
///
/// Tracers terminate when they run out of work and can't steal work from other
/// tracers. Any attempt at implementing a retry mechanism of sorts lead to
/// worse tracing performance, so we instead just let tracers terminate.
pub struct Tracer {
    /// The local queue of objects to trace.
    queue: Worker<ObjectPointerPointer>,

    /// The pool of tracers this Tracer belongs to.
    pool: ArcWithoutWeak<Pool>,
}

impl Tracer {
    pub fn new(
        queue: Worker<ObjectPointerPointer>,
        pool: ArcWithoutWeak<Pool>,
    ) -> Self {
        Self { queue, pool }
    }

    /// Traces through all live objects, without moving any objects.
    pub fn trace_without_moving(&self) -> TraceStatistics {
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
    pub fn trace_with_moving(&self) -> TraceStatistics {
        let mut stats = TraceStatistics::new();

        while let Some(pointer_pointer) = self.pop_job() {
            let pointer = pointer_pointer.get_mut();

            if pointer.is_marked() {
                continue;
            }

            match pointer.status() {
                ObjectStatus::Resolve => pointer.resolve_forwarding_pointer(),
                ObjectStatus::Promote => {
                    self.promote_mature(pointer);

                    stats.promoted += 1;
                    stats.marked += 1;

                    pointer.mark();

                    // When promoting an object we already trace it, so we
                    // don't need to trace it again below.
                    continue;
                }
                ObjectStatus::Evacuate => {
                    self.evacuate(pointer);

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
    fn trace_promoted_object(&self, promoted: ObjectPointer) {
        let mut remember = false;

        promoted.get().each_pointer(|child| {
            if !remember && child.get().is_young() {
                self.pool.process.remember_object(promoted);

                remember = true;
            }

            self.queue.push(child);
        });
    }

    /// Promotes an object to the mature generation.
    ///
    /// The pointer to promote is updated to point to the new location.
    fn promote_mature(&self, pointer: &mut ObjectPointer) {
        let local_data = self.pool.process.local_data_mut();
        let old_obj = pointer.get_mut();
        let new_pointer = local_data.allocator.allocate_mature(old_obj.take());

        old_obj.forward_to(new_pointer);

        pointer.resolve_forwarding_pointer();

        self.trace_promoted_object(*pointer);
    }

    // Evacuates a pointer.
    //
    // The pointer to evacuate is updated to point to the new location.
    fn evacuate(&self, pointer: &mut ObjectPointer) {
        // When evacuating an object we must ensure we evacuate the object into
        // the same bucket.
        let local_data = self.pool.process.local_data_mut();
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
            match self.pool.global_queue.steal_batch_and_pop(&self.queue) {
                Steal::Retry => {}
                Steal::Empty => break,
                Steal::Success(job) => return Some(job),
            };
        }

        // We don't steal in random order, as we found that stealing in-order
        // performs better.
        for stealer in self.pool.stealers.iter() {
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

    fn prepare(process: RcProcess) -> (ArcWithoutWeak<Pool>, Tracer) {
        let (pool, mut tracers) = Pool::new(process, 1);

        (pool, tracers.pop().unwrap())
    }

    #[test]
    fn test_promote_mature() {
        let (_machine, _block, process) = setup();
        let (_pool, tracer) = prepare(process.clone());
        let mut pointer =
            process.allocate_without_prototype(object_value::float(15.0));

        let old_address = pointer.raw.raw as usize;

        tracer.promote_mature(&mut pointer);

        let new_address = pointer.raw.raw as usize;

        assert!(old_address != new_address);
        assert!(pointer.is_mature());
        assert_eq!(pointer.float_value().unwrap(), 15.0);
    }

    #[test]
    fn test_evacuate() {
        let (_machine, _block, process) = setup();
        let (_pool, tracer) = prepare(process.clone());
        let mut pointer =
            process.allocate_without_prototype(object_value::float(15.0));

        let old_address = pointer.raw.raw as usize;

        tracer.evacuate(&mut pointer);

        let new_address = pointer.raw.raw as usize;

        assert!(old_address != new_address);
        assert_eq!(pointer.float_value().unwrap(), 15.0);
    }

    #[test]
    fn test_trace_with_moving_with_marked_mature() {
        let (_machine, _block, process) = setup();
        let (pool, tracer) = prepare(process.clone());
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

        let stats = tracer.trace_with_moving();

        assert_eq!(stats.marked, 2);
        assert_eq!(stats.evacuated, 2);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_with_moving_with_unmarked_mature() {
        let (_machine, _block, process) = setup();
        let (pool, tracer) = prepare(process.clone());
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

        let stats = tracer.trace_with_moving();

        assert_eq!(stats.marked, 3);
        assert_eq!(stats.evacuated, 3);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_without_moving_with_marked_mature() {
        let (_machine, _block, process) = setup();
        let (pool, tracer) = prepare(process.clone());
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

        let stats = tracer.trace_without_moving();

        assert!(young_parent.is_marked());
        assert!(young_child.is_marked());

        assert_eq!(stats.marked, 2);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);
    }

    #[test]
    fn test_trace_without_moving_with_unmarked_mature() {
        let (_machine, _block, process) = setup();
        let (pool, tracer) = prepare(process.clone());
        let young_parent = process.allocate_empty();
        let young_child = process.allocate_empty();

        young_parent.add_attribute(&process, young_child, young_child);

        let mature = process
            .local_data_mut()
            .allocator
            .allocate_mature(Object::new(object_value::none()));

        pool.schedule(young_parent.pointer());
        pool.schedule(mature.pointer());

        let stats = tracer.trace_without_moving();

        assert!(young_parent.is_marked());
        assert!(young_child.is_marked());
        assert!(mature.is_marked());

        assert_eq!(stats.marked, 3);
        assert_eq!(stats.evacuated, 0);
        assert_eq!(stats.promoted, 0);
    }
}
