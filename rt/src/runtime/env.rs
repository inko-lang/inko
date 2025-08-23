use crate::mem::PrimitiveString;
use crate::state::State;
use std::env;
use std::path::PathBuf;

#[no_mangle]
pub unsafe extern "system" fn inko_env_get(
    state: *const State,
    name: PrimitiveString,
) -> PrimitiveString {
    (*state)
        .environment
        .get(name.as_str())
        .cloned()
        .map(PrimitiveString::owned)
        .unwrap_or_else(PrimitiveString::empty)
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_get_key(
    state: *const State,
    index: i64,
) -> PrimitiveString {
    // This is only used to populate a map of all variables, and for that we'll
    // only use indexes that actually exist, so we can just unwrap here instead
    // of returning a result value.
    PrimitiveString::borrowed((*state).environment.key(index as _).unwrap())
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_size(state: *const State) -> i64 {
    (*state).environment.len() as _
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_temp_directory() -> PrimitiveString {
    let path = canonalize(env::temp_dir().to_string_lossy().into_owned());

    PrimitiveString::owned(path)
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
) -> PrimitiveString {
    PrimitiveString::borrowed(
        (&(*state)).arguments.get_unchecked(index as usize),
    )
}

#[no_mangle]
pub unsafe extern "system" fn inko_env_executable() -> PrimitiveString {
    env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .map(PrimitiveString::owned)
        .unwrap_or_else(PrimitiveString::error)
}

fn canonalize(path: String) -> String {
    PathBuf::from(&path)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(path)
}
