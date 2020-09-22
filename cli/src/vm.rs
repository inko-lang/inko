//! Functions for interacting with the Inko VM.
use libinko::config::Config;
use libinko::vm::machine::Machine;
use libinko::vm::state::State;

pub fn start(path: &str, arguments: &[String]) -> i32 {
    let mut config = Config::new();

    config.populate_from_env();

    let machine = Machine::new(State::with_rc(config, arguments));

    machine.start(path);
    machine.state.current_exit_status()
}
