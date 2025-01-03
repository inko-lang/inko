use crate::mem::String as InkoString;
use crate::result::Result as InkoResult;
use crate::state::State;
use std::env;
use std::path::PathBuf;

#[no_mangle]
pub unsafe extern "system" fn inko_env_get(
    state: *const State,
    name: *const InkoString,
) -> InkoResult {
    let state = &(*state);
    let name = InkoString::read(name);

    state
        .environment
        .get(name)
        .cloned()
        .map(|v| InkoResult::ok(InkoString::alloc(state.string_type, v) as _))
        .unwrap_or_else(InkoResult::none)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_get_key(
    state: *const State,
    index: i64,
) -> *const InkoString {
    let state = &(*state);

    // This is only used to populate a map of all variables, and for that we'll
    // only use indexes that actually exist, so we can just unwrap here instead
    // of returning a result value.
    let val = state.environment.key(index as _).unwrap().clone();

    InkoString::alloc(state.string_type, val)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_size(state: *const State) -> i64 {
    (*state).environment.len() as _
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_temp_directory(
    state: *const State,
) -> *const InkoString {
    let path = canonalize(env::temp_dir().to_string_lossy().into_owned());

    InkoString::alloc((*state).string_type, path)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_get_working_directory(
    state: *const State,
) -> InkoResult {
    env::current_dir()
        .map(|path| canonalize(path.to_string_lossy().into_owned()))
        .map(|path| {
            InkoResult::ok(InkoString::alloc((*state).string_type, path) as _)
        })
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_set_working_directory(
    directory: *const InkoString,
) -> InkoResult {
    let dir = InkoString::read(directory);

    env::set_current_dir(dir)
        .map(|_| InkoResult::none())
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_arguments_size(
    state: *const State,
) -> i64 {
    (*state).arguments.len() as i64
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_argument(
    state: *const State,
    index: i64,
) -> *const InkoString {
    let state = &(*state);

    InkoString::alloc(
        state.string_type,
        state.arguments.get_unchecked(index as usize).clone(),
    )
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_executable(
    state: *const State,
) -> InkoResult {
    env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .map(|path| {
            InkoResult::ok(InkoString::alloc((*state).string_type, path) as _)
        })
        .unwrap_or_else(InkoResult::io_error)
}

fn canonalize(path: String) -> String {
    PathBuf::from(&path)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(path)
}
