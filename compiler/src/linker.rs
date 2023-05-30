use crate::config::Config;
use crate::target::OperatingSystem;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn runtime_library(config: &Config) -> Option<PathBuf> {
    let mut files = vec![format!("libinko-{}.a", &config.target)];

    // When compiling for the native target we also support DIR/libinko.a, as
    // this makes development of Inko easier by just using e.g. `./target/debug`
    // as the search directory.
    if config.target.is_native() {
        files.push("libinko.a".to_string());
    }

    files.iter().find_map(|file| {
        let path = config.runtime.join(file);

        if path.is_file() {
            Some(path)
        } else {
            None
        }
    })
}

fn lld_is_available() -> bool {
    Command::new("ld.lld")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .and_then(|mut child| child.wait())
        .map_or(false, |status| status.success())
}

pub(crate) fn link(
    config: &Config,
    output: &Path,
    paths: &[PathBuf],
) -> Result<(), String> {
    // On Unix systems the necessary libraries/object files are all over the
    // place. Instead of re-implementing the logic necessary to find these
    // files, we rely on the system's compiler to do this for us.
    //
    // As we only use this executable for linking it doesn't really matter
    // if this ends up using gcc, clang or something else, because we only
    // use it as a wrapper around the linker executable.
    let mut cmd = Command::new("cc");

    // Object files must come before any of the libraries to link against, as
    // certain linkers are very particular about the order of flags such as
    // `-l`.
    for path in paths {
        cmd.arg(path);
    }

    match config.target.os {
        OperatingSystem::Linux
        | OperatingSystem::Freebsd
        | OperatingSystem::Openbsd => {
            // macOS includes libm in the standard C library, so there's no need
            // to explicitly include it.
            cmd.arg("-lm");
        }
        _ => {}
    }

    cmd.arg("-o");
    cmd.arg(output);

    if let OperatingSystem::Linux = config.target.os {
        // This removes the need for installing libgcc in deployment
        // environments.
        cmd.arg("-static-libgcc");

        // On platforms where lld isn't the default (e.g. Linux), we'll use it
        // if available, speeding up the linking process.
        if lld_is_available() {
            cmd.arg("-fuse-ld=lld");
        }
    }

    let rt_path = runtime_library(config).ok_or_else(|| {
        format!("No runtime is available for target '{}'", config.target)
    })?;

    cmd.arg(&rt_path);

    cmd.stdin(Stdio::null());
    cmd.stderr(Stdio::piped());
    cmd.stdout(Stdio::null());

    let child = cmd
        .spawn()
        .map_err(|err| format!("Failed to start the linker: {err}"))?;

    let output = child
        .wait_with_output()
        .map_err(|err| format!("Failed to wait for the linker: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "The linker exited with status code {}:\n{}",
            output.status.code().unwrap_or(0),
            String::from_utf8_lossy(&output.stderr),
        ))
    }
}
