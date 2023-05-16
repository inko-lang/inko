use crate::mem::{Array, String as InkoString};
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
        .map(|path| InkoResult::ok(path as _))
        .unwrap_or_else(InkoResult::none)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_variables(
    state: *const State,
) -> *mut Array {
    let state = &*state;
    let names = state
        .environment
        .keys()
        .map(|key| {
            InkoString::alloc(state.string_class, key.clone()) as *mut u8
        })
        .collect();

    Array::alloc(state.array_class, names)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_home_directory(
    state: *const State,
) -> InkoResult {
    let state = &*state;

    // Rather than performing all sorts of magical incantations to get the home
    // directory, we're just going to require that HOME is set.
    //
    // If the home is explicitly set to an empty string we still ignore it,
    // because there's no scenario in which Some("") is useful.
    state
        .environment
        .get("HOME")
        .cloned()
        .filter(|&path| !InkoString::read(path).is_empty())
        .map(|path| InkoResult::ok(path as _))
        .unwrap_or_else(InkoResult::none)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_temp_directory(
    state: *const State,
) -> *const InkoString {
    let path = canonalize(env::temp_dir().to_string_lossy().into_owned());

    InkoString::alloc((*state).string_class, path)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_get_working_directory(
    state: *const State,
) -> InkoResult {
    env::current_dir()
        .map(|path| canonalize(path.to_string_lossy().into_owned()))
        .map(|path| {
            InkoResult::ok(InkoString::alloc((*state).string_class, path) as _)
        })
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_set_working_directory(
    state: *const State,
    directory: *const InkoString,
) -> InkoResult {
    let state = &*state;
    let dir = InkoString::read(directory);

    env::set_current_dir(dir)
        .map(|_| InkoResult::ok(state.nil_singleton as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_arguments(
    state: *const State,
) -> *mut Array {
    let state = &*state;

    Array::alloc(
        state.array_class,
        state.arguments.iter().map(|&v| v as _).collect(),
    )
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_executable(
    state: *const State,
) -> InkoResult {
    env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .map(|path| {
            InkoResult::ok(InkoString::alloc((*state).string_class, path) as _)
        })
        .unwrap_or_else(InkoResult::io_error)
}

fn canonalize(path: String) -> String {
    PathBuf::from(&path)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(path)
}
