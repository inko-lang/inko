//! Functions for performing garbage collection of a process mailbox.

use gc::collector;
use gc::profile::Profile;
use gc::trace_result::TraceResult;
use mailbox::Mailbox;
use process::RcProcess;
use vm::state::RcState;

pub fn collect(vm_state: &RcState, process: &RcProcess) -> Profile {
    process.request_gc_suspension();

    let mut profile = Profile::mailbox();

    profile.total.start();

    let mut local_data = process.local_data_mut();
    let ref mut mailbox = local_data.mailbox;

    profile.prepare.start();

    let lock = mailbox.write_lock.lock();
    let move_objects = mailbox.allocator.prepare_for_collection();

    profile.prepare.stop();
    profile.trace.start();

    let trace_result = trace(&process, &mailbox, move_objects);

    profile.trace.stop();
    profile.reclaim.start();

    mailbox.allocator.reclaim_blocks();
    process.update_mailbox_collection_statistics(&vm_state.config);
    drop(lock); // unlock as soon as possible

    profile.reclaim.stop();
    profile.total.stop();

    profile.populate_tracing_statistics(trace_result);

    vm_state.process_pools.schedule(process.clone());

    profile
}

pub fn trace(process: &RcProcess,
             mailbox: &Mailbox,
             move_objects: bool)
             -> TraceResult {
    let roots = mailbox.mailbox_pointers();

    if move_objects {
        collector::trace_pointers_with_moving(process, roots, false)
    } else {
        collector::trace_pointers_without_moving(roots, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;
    use compiled_code::CompiledCode;
    use immix::global_allocator::GlobalAllocator;
    use immix::permanent_allocator::PermanentAllocator;
    use process::{Process, RcProcess};
    use vm::state::State;

    fn new_process() -> (Box<PermanentAllocator>, RcProcess) {
        let global_alloc = GlobalAllocator::without_preallocated_blocks();

        let mut perm_alloc =
            Box::new(PermanentAllocator::new(global_alloc.clone()));

        let code = CompiledCode::with_rc("a".to_string(),
                                         "a".to_string(),
                                         1,
                                         Vec::new());

        (perm_alloc, Process::from_code(1, 0, code, global_alloc))
    }

    #[test]
    fn test_collect() {
        let (_perm_alloc, process) = new_process();
        let state = State::new(Config::new());

        let mut local_data = process.local_data_mut();

        local_data.mailbox.send_from_external(process.allocate_empty());

        local_data.mailbox.allocator.prepare_for_collection();

        let profile = collect(&state, &process);

        assert!(local_data.mailbox.external[0].is_marked());

        assert_eq!(profile.marked, 1);
        assert_eq!(profile.evacuated, 0);
        assert_eq!(profile.promoted, 0);
    }

    #[test]
    fn test_trace_without_moving() {
        let (_perm_alloc, process) = new_process();

        let mut local_data = process.local_data_mut();

        local_data.mailbox.send_from_external(process.allocate_empty());

        local_data.mailbox.allocator.prepare_for_collection();

        let result = trace(&process, &local_data.mailbox, false);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_with_moving() {
        let (_perm_alloc, process) = new_process();

        let mut local_data = process.local_data_mut();

        local_data.mailbox.send_from_external(process.allocate_empty());

        local_data.mailbox.external[0].block_mut().fragmented = true;

        local_data.mailbox.allocator.prepare_for_collection();

        let result = trace(&process, &local_data.mailbox, true);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 1);
        assert_eq!(result.promoted, 0);
    }
}
