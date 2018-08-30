//! Functions for performing garbage collection of a process mailbox.
use gc::collector;
use gc::profile::Profile;
use gc::trace_result::TraceResult;
use mailbox::Mailbox;
use process::RcProcess;
use vm::state::RcState;

pub fn collect(vm_state: &RcState, process: &RcProcess, profile: &mut Profile) {
    let local_data = process.local_data_mut();
    let mailbox = &mut local_data.mailbox;

    profile.prepare.start();

    let lock = mailbox.write_lock.lock();
    let move_objects = mailbox.allocator.prepare_for_collection();

    profile.prepare.stop();
    profile.trace.start();

    let trace_result = trace(&process, &mailbox, move_objects);

    profile.trace.stop();
    profile.reclaim.start();

    mailbox.allocator.reclaim_blocks(vm_state);
    process.update_mailbox_collection_statistics();

    drop(lock); // unlock as soon as possible

    profile.reclaim.stop();
    profile.suspended.stop();

    vm_state.process_pools.schedule(process.clone());

    profile.total.stop();
    profile.populate_tracing_statistics(&trace_result);
}

pub fn trace(
    process: &RcProcess,
    mailbox: &Mailbox,
    move_objects: bool,
) -> TraceResult {
    let roots = unsafe { mailbox.mailbox_pointers() };

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
    use vm::state::State;
    use vm::test::setup;

    #[test]
    fn test_collect() {
        let (_machine, _block, process) = setup();
        let state = State::new(Config::new(), &[]);
        let mut profile = Profile::young();
        let local_data = process.local_data_mut();

        local_data
            .mailbox
            .send_from_external(process.allocate_empty());

        local_data.mailbox.allocator.prepare_for_collection();

        collect(&state, &process, &mut profile);

        assert!(
            local_data
                .mailbox
                .external
                .iter()
                .next()
                .unwrap()
                .is_marked()
        );

        assert_eq!(profile.marked, 1);
        assert_eq!(profile.evacuated, 0);
        assert_eq!(profile.promoted, 0);
    }

    #[test]
    fn test_trace_without_moving() {
        let (_machine, _block, process) = setup();

        let local_data = process.local_data_mut();

        local_data
            .mailbox
            .send_from_external(process.allocate_empty());

        local_data.mailbox.allocator.prepare_for_collection();

        let result = trace(&process, &local_data.mailbox, false);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 0);
        assert_eq!(result.promoted, 0);
    }

    #[test]
    fn test_trace_with_moving() {
        let (_machine, _block, process) = setup();

        let local_data = process.local_data_mut();

        local_data
            .mailbox
            .send_from_external(process.allocate_empty());

        local_data
            .mailbox
            .external
            .iter_mut()
            .next()
            .unwrap()
            .block_mut()
            .set_fragmented();

        local_data.mailbox.allocator.prepare_for_collection();

        let result = trace(&process, &local_data.mailbox, true);

        assert_eq!(result.marked, 1);
        assert_eq!(result.evacuated, 1);
        assert_eq!(result.promoted, 0);
    }
}
