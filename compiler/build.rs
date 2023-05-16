use std::env;
use std::path::PathBuf;

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
    let profile = env::var("PROFILE").unwrap();
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
