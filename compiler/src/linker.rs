use crate::config::{Config, Linker};
use crate::state::State;
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

fn linker_is_available(linker: &str) -> bool {
    Command::new(linker)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .and_then(|mut child| child.wait())
        .map_or(false, |status| status.success())
}

fn lld_is_available() -> bool {
    linker_is_available("ld.lld")
}

fn mold_is_available() -> bool {
    linker_is_available("ld.mold")
}

pub(crate) fn link(
    state: &State,
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

    let rt_path = runtime_library(&state.config).ok_or_else(|| {
        format!("No runtime is available for target '{}'", state.config.target)
    })?;

    cmd.arg(&rt_path);

    // Include any extra platform specific libraries, such as libm on the
    // various Unix platforms. These must come _after_ any object files and
    // the runtime library path.
    //
    // macOS includes libm in the standard C library, so there's no need to
    // explicitly include it.
    //
    // We don't support static linking as libm is part of glibc, libc doesn't
    // really support (proper) static linking, and you can't statically link
    // libm _without_ statically linking libc. See
    // https://bugzilla.redhat.com/show_bug.cgi?id=1433347 for some extra
    // details.
    match state.config.target.os {
        OperatingSystem::Linux => {
            // Certain versions of Linux (e.g. Debian 11) also need libdl and
            // libpthread to be linked in explicitly. We use the --as-needed
            // flag here (supported by both gcc and clang) to only link these
            // libraries if actually needed.
            cmd.arg("-Wl,--as-needed");
            cmd.arg("-ldl");
            cmd.arg("-lm");
            cmd.arg("-lpthread");
        }
        OperatingSystem::Freebsd => {
            cmd.arg("-lm");
            cmd.arg("-lpthread");
        }
        _ => {}
    }

    let mut static_linking = state.config.static_linking;

    match state.config.target.os {
        OperatingSystem::Mac if static_linking => {
            // On macOS there's no equivalent of -l:libX.a as there is for GNU
            // platforms. We also don't have the logic (nor want to implement this)
            // to find where the .a files are for each library linked against.
            println!(
                "Static linking isn't supported on macOS, \
                falling back to dynamic linking"
            );

            static_linking = false;
        }
        _ => (),
    }

    if static_linking {
        cmd.arg("-Wl,-Bstatic");
    }

    for lib in &state.libraries {
        // These libraries are already included if needed, and we can't
        // statically link against them (if static linking is desired), so we
        // skip them here.
        if lib == "m" || lib == "c" {
            continue;
        }

        cmd.arg(&(format!("-l{}", lib)));
    }

    if static_linking {
        cmd.arg("-Wl,-Bdynamic");
    }

    cmd.arg("-o");
    cmd.arg(output);

    if let OperatingSystem::Linux = state.config.target.os {
        // This removes the need for installing libgcc in deployment
        // environments.
        cmd.arg("-static-libgcc");
    }

    let mut linker = state.config.linker;

    if let Linker::Detect = linker {
        if mold_is_available() {
            linker = Linker::Mold;
        } else if lld_is_available() {
            linker = Linker::Lld;
        }
    }

    match linker {
        Linker::Lld => {
            cmd.arg("-fuse-ld=lld");
        }
        Linker::Mold => {
            cmd.arg("-fuse-ld=mold");
        }
        _ => {}
    }

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
