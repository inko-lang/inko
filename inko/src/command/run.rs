use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::Config;
use getopts::{Options, ParsingStyle};
use std::env::temp_dir;
use std::fs::{create_dir, remove_dir_all};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
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

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut config = Config::default();
    let arguments =
        if matches.free.len() > 1 { &matches.free[1..] } else { &[] };

    if let Some(format) = matches.opt_str("f") {
        config.set_presenter(&format)?;
    }

    for path in matches.opt_strs("i") {
        config.add_source_directory(path.into());
    }

    if matches.opt_present("static") {
        config.static_linking = true;
    }

    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();
    let build_dir = temp_dir().join(format!("inko-run-{}", time));

    if !build_dir.is_dir() {
        create_dir(&build_dir).map_err(|err| {
            Error::from(format!(
                "Failed to create {}: {}",
                build_dir.display(),
                err
            ))
        })?;
    }

    config.build = build_dir.clone();

    let mut compiler = Compiler::new(config);
    let file = matches.free.first().map(PathBuf::from);
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
                        "Failed to run the executable: {}",
                        err
                    ))
                })
                .map(|status| status.code().unwrap_or(0));

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
