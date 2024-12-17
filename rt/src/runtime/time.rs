use crate::state::State;

#[no_mangle]
pub unsafe extern "system" fn inko_time_monotonic(state: *const State) -> i64 {
    // An i64 gives us roughly 292 years of time. That should be more than
    // enough for a monotonic clock, as an Inko program is unlikely to run for
    // that long.
    let state = &*state;

    state.start_time.elapsed().as_nanos() as i64
}
