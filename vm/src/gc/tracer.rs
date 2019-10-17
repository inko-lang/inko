//! Tracing and marking of live objects.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::gc::statistics::TraceStatistics;
use crate::object::ObjectStatus;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use crate::process::RcProcess;
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use std::sync::atomic::{spin_loop_hint, AtomicUsize, Ordering};

/// The raw tracing loop, with the part for actually tracing and marking being
/// supplied as an argument.
///
/// This macro is used to implement the two tracing loops (non-moving and
/// moving), without having to duplicate the code manually.
///
/// The tracing loop terminates automatically once all workers run out of work,
/// and takes care of not terminating threads prematurely.
///
/// Much of the work of this macro is delegated to separate methods, as
/// otherwise we'd end up with quite a few nested loops; which gets hard to
/// read.
macro_rules! trace_loop {
    ($self: expr, $work: expr) => {
        loop {
            $self.steal_from_global();

            $work;

            if $self.has_global_jobs() {
                continue;
            }

            $self.set_idle();

            if $self.steal_from_worker() {
                $self.set_busy();
                continue;
            }

            // Depending on how fast a thread runs, a thread may reach this
            // point while there is still work left to be done. For example, the
            // following series of events can take place:
            //
            // 1. Thread A is working.
            // 2. Thread B is spinning in the "while" above, trying
            //    to steal work.
            // 3. Thread B steals work from A.
            // 4. Thread A runs out of work.
            // 5. Thread A enters the "while" loop and observes all
            //    worker sto be idle.
            // 6. Thread A reaches this point.
            // 7. Thread B increments "busy" and restarts its loop,
            //    processing the work it stole earlier.
            //
            // To prevent thread A from terminating when new work may be
            // produced, we double check both the queue sizes and the number of
            // busy workers. Just checking the queue sizes is not enough. If a
            // worker popped a job and is still processing it, its queue might
            // be empty but new work may still be produced.
            //
            // Since the "busy" counter is incremented _before_ a worker starts,
            // checking both should ensure we never terminate a thread until we
            // are certain all work has been completed.
            if $self.should_terminate() {
                break;
            }

            $self.set_busy();
        }
    };
}

/// A pool of Tracers all tracing the same process.
pub struct Pool {
    /// The process of which objects are being traced.
    process: RcProcess,

    /// A global queue to steal jobs from.
    global_queue: Injector<ObjectPointerPointer>,

    /// The list of queues we can steal work from.
    stealers: Vec<Stealer<ObjectPointerPointer>>,

    /// An integer storing the number of busy tracers in a pool.
    busy: AtomicUsize,
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
            busy: AtomicUsize::new(threads),
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

        trace_loop!(
            self,
            while let Some(pointer_pointer) = self.queue.pop() {
                let pointer = pointer_pointer.get();

                if pointer.is_marked() {
                    continue;
                }

                pointer.mark();

                stats.marked += 1;

                pointer.get().each_pointer(|child| {
                    self.queue.push(child);
                });
            }
        );

        stats
    }

    /// Traces through all live objects, moving them if needed.
    pub fn trace_with_moving(&self) -> TraceStatistics {
        let mut stats = TraceStatistics::new();

        trace_loop!(
            self,
            while let Some(pointer_pointer) = self.queue.pop() {
                let pointer = pointer_pointer.get_mut();

                if pointer.is_marked() {
                    continue;
                }

                match pointer.status() {
                    ObjectStatus::Resolve => {
                        pointer.resolve_forwarding_pointer()
                    }
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

                pointer.get().each_pointer(|child| {
                    self.queue.push(child);
                });
            }
        );

        stats
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

    fn steal_from_global(&self) {
        loop {
            match self.pool.global_queue.steal_batch(&self.queue) {
                Steal::Empty | Steal::Success(_) => break,
                Steal::Retry => {}
            };

            spin_loop_hint();
        }
    }

    fn steal_from_worker(&self) -> bool {
        while self.pool.busy.load(Ordering::Acquire) > 0 {
            for stealer in self.pool.stealers.iter() {
                loop {
                    match stealer.steal_batch(&self.queue) {
                        Steal::Empty => break,
                        Steal::Retry => {}
                        Steal::Success(_) => return true,
                    }
                }
            }

            spin_loop_hint();
        }

        false
    }

    fn should_terminate(&self) -> bool {
        self.pool.stealers.iter().all(|x| x.is_empty())
            && self.pool.busy.load(Ordering::Acquire) == 0
    }

    fn has_global_jobs(&self) -> bool {
        !self.pool.global_queue.is_empty()
    }

    fn set_busy(&self) {
        self.pool.busy.fetch_add(1, Ordering::Release);
    }

    fn set_idle(&self) {
        self.pool.busy.fetch_sub(1, Ordering::Release);
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
