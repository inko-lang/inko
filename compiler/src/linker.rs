use crate::config::{local_runtimes_directory, BuildDirectories, Linker};
use crate::state::State;
use crate::target::{OperatingSystem, Target, MAC_SDK_VERSION};
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn command_is_available(name: &str) -> bool {
    // We use --help here instead of --version, as not all commands may have a
    // --version flag (e.g. "zig" for some reason).
    Command::new(name)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .and_then(|mut child| child.wait())
        .map_or(false, |status| status.success())
}

fn cc_is_clang() -> bool {
    let Ok(mut child) = Command::new("cc")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
    else {
        return false;
    };

    let mut stdout = child.stdout.take().unwrap();
    let Ok(status) = child.wait() else { return false };
    let mut output = String::new();
    let _ = stdout.read_to_string(&mut output);

    status.success() && output.contains("clang version")
}

fn lld_is_available() -> bool {
    command_is_available("ld.lld")
}

fn mold_is_available() -> bool {
    command_is_available("ld.mold")
}

fn musl_linker(target: &Target) -> Option<&'static str> {
    if !target.abi.is_musl() {
        return None;
    }

    let gcc = "musl-gcc";
    let clang = "musl-clang";

    if command_is_available(gcc) {
        Some(gcc)
    } else if command_is_available(clang) {
        Some(clang)
    } else {
        None
    }
}

fn zig_cc(target: &Target) -> Command {
    let mut cmd = Command::new("zig");

    // To make using Zig as a linker a bit easier, we translate our target
    // triples into those used by Zig (which in turn are a bit different
    // from the ones used by LLVM).
    cmd.arg("cc");
    cmd.arg(&format!("--target={}", target.zig_triple()));
    cmd
}

fn driver(state: &State) -> Result<Command, String> {
    let target = &state.config.target;
    let triple = target.llvm_triple();
    let cmd = if let Linker::Custom(name) = &state.config.linker {
        Command::new(name)
    } else if state.config.linker.is_zig() {
        zig_cc(target)
    } else {
        let gcc_exe = format!("{}-gcc", triple);
        let mut linker = state.config.linker.clone();
        let mut cmd = if target.is_native() {
            Command::new("cc")
        } else if target.os.is_mac() && command_is_available("zig") {
            // Cross-compiling from a non-mac host to macOS is a pain due to the
            // licensing of the various dependencies needed for this. Zig makes
            // this much easier, so we'll use it if it's available.
            linker = Linker::System;
            zig_cc(target)
        } else if command_is_available(&gcc_exe) {
            // GCC cross compilers don't support the `-fuse-ld` flag, so in this
            // case we force the use of the system linker.
            linker = Linker::System;
            Command::new(gcc_exe)
        } else if let Some(name) = musl_linker(target) {
            // For musl we want to use the musl-gcc/musl-clang wrappers so we
            // don't have to figure out all the necessary parameters ourselves.
            linker = Linker::System;
            Command::new(name)
        } else if cc_is_clang() || command_is_available("clang") {
            // We check for clang _after_ GCC, because if a dedicated GCC
            // executable for the target is available, using it is less prone to
            // error as we don't have to bother finding the right sysroot.
            Command::new("clang")
        } else {
            return Err(format!(
                "you are cross-compiling to {}, but the linker used (cc) \
                doesn't support cross-compilation. You can specify a custom \
                linker using the --linker=LINKER option",
                triple
            ));
        };

        if cmd.get_program() == "clang" {
            // clang tends to pick the host version of any necessary libraries
            // (including crt1.o and the likes) when cross-compiling. We try to
            // fix this here by automatically setting the correct sysroot.
            //
            // Linux distributions (Arch Linux, Fedora, Ubuntu, etc) typically
            // install the toolchains in /usr, e.g. /usr/aarch64-linux-gnu.
            //
            // For other platforms we don't bother trying to find the sysroot,
            // as they don't reside in a consistent location.
            if cfg!(target_os = "linux") {
                let path = format!("/usr/{}", triple);

                if Path::new(&path).is_dir() {
                    cmd.arg(format!("--sysroot={}", path));
                }
            }

            cmd.arg(&format!("--target={}", triple));
        }

        if let Linker::Detect = linker {
            // Mold doesn't support macOS, so we don't enable it for macOS
            // targets.
            if mold_is_available() && !target.os.is_mac() {
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

        cmd
    };

    let name = cmd.get_program();

    // While it's possible the user has specified the many flags needed to get
    // regular clang/gcc to correctly use musl, it's rather unlikely, so we
    // instead just direct users to use musl-clang, musl-gcc, or zig and be done
    // with it.
    if cfg!(target_env = "gnu")
        && target.abi.is_musl()
        && name != "musl-clang"
        && name != "musl-gcc"
        && name != "zig"
    {
        return Err(
            "targeting musl on a GNU host using clang or gcc is likely to \
            result in the executable still linking against glibc. To resolve \
            this, use --linker=zig, or --linker=musl-clang/--linker=musl-gcc \
            (provided musl-clang/musl-gcc is in your PATH)"
                .to_string(),
        );
    }

    Ok(cmd)
}

pub(crate) fn link(
    state: &State,
    output: &Path,
    paths: &[PathBuf],
    directories: &BuildDirectories,
) -> Result<(), String> {
    let mut cmd = driver(state)?;

    for arg in &state.config.linker_arguments {
        cmd.arg(arg);
    }

    // Create a response file for the linker to allow linking large numbers of object files.
    // For more details, refer to https://github.com/inko-lang/inko/issues/595.
    let rsp = directories.objects.join("link.rsp");
    File::create(&rsp)
        .and_then(|mut file| {
            let content = paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n");

            file.write_all(content.as_bytes())?;

            cmd.arg(format!("@{}", rsp.display()));

            Ok(())
        })
        .map_err(|e| {
            format!("failed to write objects to {}: {}", rsp.display(), e)
        })?;

    if state.config.target.is_native() {
        cmd.arg(state.config.runtime.join("libinko.a"));
    } else if let Some(runtimes) = local_runtimes_directory() {
        let dir = runtimes.join(state.config.target.to_string());
        let inko = dir.join("libinko.a");
        let unwind = dir.join("libunwind.a");

        if !inko.is_file() {
            return Err(format!(
                "no runtime is available for target '{}'",
                state.config.target
            ));
        }

        cmd.arg(inko);

        // On musl hosts we just rely on whatever the system unwinder is. This
        // way distributions such as Alpine don't need to patch things out to
        // achieve that. On other hosts we use the bundled libunwind, otherwise
        // we get _Unwind_XXX linker errors.
        if !cfg!(target_env = "musl")
            && state.config.target.abi.is_musl()
            && unwind.is_file()
        {
            cmd.arg(unwind);
        }
    }

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
            // flag here (supported by both GCC and clang) to only link these
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
        OperatingSystem::Mac => {
            // This is needed for TLS support.
            for name in ["Security", "CoreFoundation"] {
                cmd.arg("-framework");
                cmd.arg(name);
            }

            // This is needed by the atomic-wait crate.
            cmd.arg("-lstdc++");

            // We need to ensure that we target the same minimum version as Rust
            // uses when building the runtime library.
            cmd.arg(format!("-mmacosx-version-min={}", MAC_SDK_VERSION));
        }
    }

    let mut static_linking = state.config.static_linking;

    if !cfg!(target_env = "musl") && state.config.target.abi.is_musl() {
        // If a non-musl hosts targets musl, we statically link everything. The
        // reason for this is that when using musl one might believe the
        // resulting executable to be portable, but that's only the case if it's
        // indeed a statically linked executable.
        //
        // This does mean any C dependencies need to be available in their
        // static form, but if that's not the case then targeting musl on
        // non-musl hosts isn't going to work well anyway (e.g. because the
        // dynamic libraries are likely to link to glibc).
        static_linking = true;
    }

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

    for lib in &state.libraries {
        // These libraries are already included if needed, and we can't
        // statically link against them (if static linking is desired), so we
        // skip them here.
        if lib == "m" || lib == "c" {
            continue;
        }

        // We don't use the pattern `-Wl,-Bstatic -lX -Wl,-Bdynamic` as the
        // "closing" `-Bdynamic` also affects any linker flags that come after
        // it, which can prevent us from static linking against e.g. libc for
        // musl targets.
        let flag = if static_linking {
            format!("-l:lib{}.a", lib)
        } else {
            format!("-l{}", lib)
        };

        cmd.arg(&flag);
    }

    if state.config.target.os.is_linux() {
        // For these targets we need to ensure this flag is set, which isn't
        // always passed by GCC (and possibly other) compilers.
        cmd.arg("-Wl,--eh-frame-hdr");

        // This removes the need for installing libgcc in deployment
        // environments.
        cmd.arg("-static-libgcc");
    }

    // In case we're targeting musl we also want to statically link musl's libc.
    // This isn't done for GNU targets because glibc makes use of dlopen(), so
    // static linking glibc is basically a lie (and generally recommended
    // against). This also ensures all linkers behave the same, as e.g. Zig
    // defaults to static linking (https://github.com/ziglang/zig/issues/11909)
    // but musl-clang and musl-gcc default to dynamic linking.
    if static_linking && state.config.target.abi.is_musl() {
        cmd.arg("-static");
    }

    cmd.arg("-o");
    cmd.arg(output);

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
            "the linker exited with status code {}:\n\n{}",
            output.status.code().unwrap_or(0),
            String::from_utf8_lossy(&output.stderr).trim(),
        ))
    }
}
