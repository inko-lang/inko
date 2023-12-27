use std::env;
use std::path::{PathBuf, MAIN_SEPARATOR};

fn main() {
    // To make development easier we default to using repository-local paths.
    let rt = env::var("INKO_RT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| runtime_directory());

    let std = env::var("INKO_STD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std_directory());

    println!("cargo:rerun-if-env-changed=INKO_STD");
    println!("cargo:rerun-if-env-changed=INKO_RT");
    println!(
        "cargo:rustc-env=INKO_STD={}",
        std.canonicalize().unwrap_or(std).display(),
    );
    println!(
        "cargo:rustc-env=INKO_RT={}",
        rt.canonicalize().unwrap_or(rt).display()
    );
}

fn std_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("std")
        .join("src")
}

fn runtime_directory() -> PathBuf {
    let target = env::var("TARGET").unwrap();
    let profile = profile_name();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target");

    let without_target = base.join(&profile);
    let with_target = base.join(target).join(profile);

    if with_target.is_dir() {
        with_target
    } else {
        without_target
    }
}

/// Returns the name of the profile used, taking into account custom profile
/// names.
///
/// See the following issues for more details:
///
/// - https://stackoverflow.com/questions/73595435/how-to-get-profile-from-cargo-toml-in-build-rs-or-at-runtime
/// - https://github.com/rust-lang/cargo/issues/9661
fn profile_name() -> String {
    env::var("OUT_DIR")
        .unwrap()
        .split(MAIN_SEPARATOR)
        .nth_back(3)
        .map(|v| v.to_string())
        .unwrap_or_else(|| env::var("PROFILE").unwrap())
}
