use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::{Config, Opt};
use getopts::{Options, ParsingStyle};
use std::env::temp_dir;
use std::fs::{create_dir, remove_dir_all};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

const USAGE: &str = "Usage: inko run [OPTIONS] [FILE] [ARGS]

Compile a source file and its dependencies into an executable, then run it.

Running source files is meant for development and scripting purposes, as it
requires compiling source code from scratch every time. When distributing or
deploying your Inko software, you should build it ahead of time using the
\"inko build\" command.

Arguments passed _after_ the file to run are passed to the resulting executable.

Examples:

    inko run hello.inko        # Compile and run the file hello.inko
    inko run hello.inko --foo  # Passes --foo to the resulting executable";

#[cfg(unix)]
fn exit_code(status: ExitStatus) -> Result<i32, Error> {
    if let Some(v) = status.code() {
        return Ok(v);
    }

    // 11 is SIGSEGV on pretty much every Unix platform (at least the ones we
    // care about), so we use it directly instead of depending on e.g. the libc
    // crate.
    if let Some(11) = status.signal() {
        Err(Error::from(
            "the executable was terminated by signal SIGSEGV".to_string(),
        ))
    } else {
        Ok(0)
    }
}

#[cfg(not(unix))]
fn exit_code(status: ExitStatus) -> Result<i32, Error> {
    Ok(status.code().unwrap_or(1))
}

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("", "release", "Perform a release build");
    options.optflag("h", "help", "Show this help message");
    options.optopt(
        "f",
        "format",
        "The output format to use for diagnostics",
        "FORMAT",
    );

    options.optmulti(
        "i",
        "include",
        "A directory to add to the list of source directories",
        "PATH",
    );

    options.optflag("", "static", "Statically link imported C libraries");
    options.optopt(
        "",
        "directory",
        "A custom name for the temporary build directory",
        "NAME",
    );

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut config = Config::default();
    let arguments =
        if matches.free.len() > 1 { &matches.free[1..] } else { &[] };

    if let Some(format) = matches.opt_str("format") {
        config.set_presenter(&format)?;
    }

    for path in matches.opt_strs("include") {
        config.add_source_directory(path.into());
    }

    if matches.opt_present("static") {
        config.static_linking = true;
    }

    if matches.opt_present("release") {
        config.opt = Opt::Release;
    }

    let dir_name = matches
        .opt_str("directory")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_secs();

            format!("inko-run-{}", time)
        });

    let build_dir = temp_dir().join(dir_name);

    if !build_dir.is_dir() {
        create_dir(&build_dir).map_err(|err| {
            Error::from(format!(
                "failed to create {}: {}",
                build_dir.display(),
                err
            ))
        })?;
    }

    config.build = build_dir.clone();

    let file = if let Some(v) = matches.free.first() {
        PathBuf::from(v)
    } else {
        return Err(Error::from("you must specify a file to run".to_string()));
    };

    let mut compiler = Compiler::new(config);
    let result = compiler.build(file);

    compiler.print_diagnostics();

    match result {
        Ok(exe) => {
            let status = Command::new(exe)
                .args(arguments)
                .spawn()
                .and_then(|mut child| child.wait())
                .map_err(|err| {
                    Error::from(format!(
                        "failed to run the executable: {}",
                        err
                    ))
                })
                .and_then(exit_code);

            if build_dir.is_dir() {
                // If this fails that dosen't matter because temporary files are
                // removed upon shutdown anyway.
                let _ = remove_dir_all(&build_dir);
            }

            status
        }
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::from(msg)),
    }
}
