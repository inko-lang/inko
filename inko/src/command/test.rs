use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::{Config, Opt, SOURCE_EXT};
use getopts::Options;
use std::fs::{read_dir, read_to_string, write};
use std::path::{Path, PathBuf};
use std::process::Command;
use types::module_name::ModuleName;

const USAGE: &str = "Usage: inko test [OPTIONS]

Compiles and runs unit tests

This command compiles your unit tests in ./test, then runs the resulting test
executable.

Examples:

    inko test    # Runs all unit tests in ./test";

/// The exit status used when one or more tests failed.
const TESTS_FAILED_STATUS: i32 = 2;

/// Compiles and runs Inko unit tests.
pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");
    options.optopt(
        "t",
        "target",
        "The target platform to compile for",
        "TARGET",
    );
    options.optflag("", "release", "Perform a release build");
    options.optflag("", "verify", "Verify build output at various stages");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut config = Config::default();

    if let Some(val) = matches.opt_str("target") {
        config.set_target(&val)?;
    }

    if matches.opt_present("release") {
        config.opt = Opt::Release;
    }

    if matches.opt_present("verify") {
        config.verify = true;
    }

    let main_file = config.main_test_module();

    if !config.tests.is_dir() {
        return Err(Error::from(format!(
            "The tests directory {:?} doesn't exist",
            config.tests
        )));
    }

    config.add_source_directory(config.tests.clone());

    let tests = test_module_names(&config.tests).map_err(|err| {
        Error::from(format!("Failed to find test modules: {}", err))
    })?;

    let mut compiler = Compiler::new(config);

    // The build/ directory needs to be created first, otherwise we can't save
    // the generated file in it (if it doesn't already exist that is).
    compiler.create_build_directory()?;

    let old_code = read_to_string(&main_file).unwrap_or_else(|_| String::new());
    let new_code = generate_main_test_module(tests);

    // We only update the file if necessary as to avoid unnecessary
    // recompilations of object files.
    if old_code != new_code {
        write(&main_file, new_code).map_err(|e| {
            Error::from(format!(
                "Failed to write {}: {}",
                main_file.display(),
                e
            ))
        })?;
    }

    let result = compiler.build(main_file);

    compiler.print_diagnostics();

    match result {
        Ok(exe) => {
            let status = Command::new(exe)
                .args(matches.free)
                .spawn()
                .and_then(|mut child| child.wait())
                .map_err(|err| {
                    Error::from(format!("failed to run the tests: {}", err))
                })?;

            // If the program is terminated due to e.g. a SIGSEGV, the status
            // code is missing and we default to 1. If we don't explicitly
            // handle this, the program might terminate silently in the event of
            // such a signal.
            let code = status.code().unwrap_or(1);

            match code {
                0 | TESTS_FAILED_STATUS => Ok(code),
                _ => Err(Error::from(format!(
                    "the tests runner exited with status code {}",
                    code
                ))),
            }
        }
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::from(msg)),
    }
}

fn is_test_file(path: &Path) -> bool {
    match path.extension().and_then(|p| p.to_str()) {
        Some(SOURCE_EXT) if path.is_file() => {}
        _ => return false,
    }

    path.file_name()
        .map(|v| v.to_string_lossy())
        .map_or(false, |v| v.starts_with("test_"))
}

fn test_files(test_dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut found = Vec::new();
    let mut pending = vec![test_dir.to_owned()];

    while let Some(path) = pending.pop() {
        let entries = read_dir(&path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                pending.push(path);
                continue;
            }

            if is_test_file(&path) {
                found.push(path);
            }
        }
    }

    Ok(found)
}

fn test_module_names(
    test_dir: &Path,
) -> Result<Vec<ModuleName>, std::io::Error> {
    let test_modules = test_files(test_dir)?
        .into_iter()
        .map(|file| {
            ModuleName::from_relative_path(file.strip_prefix(test_dir).unwrap())
        })
        .collect::<Vec<_>>();

    Ok(test_modules)
}

fn generate_main_test_module(tests: Vec<ModuleName>) -> String {
    let mut imports = Vec::with_capacity(tests.len());
    let mut calls = Vec::with_capacity(tests.len());

    for (idx, test) in tests.iter().enumerate() {
        imports.push(format!("import {} (self as tests{})\n", test, idx));
        calls.push(format!("    tests{}.tests(tests)\n", idx));
    }

    let mut source =
        "import std.env\nimport std.test (Filter, Tests)\n".to_string();

    for line in imports {
        source.push_str(&line);
    }

    source.push_str("\ntype async Main {\n");
    source.push_str("  fn async main {\n");
    source.push_str("    let tests = Tests.new\n\n");

    for line in calls {
        source.push_str(&line);
    }

    source.push_str(
        "    tests.filter = Filter.from_string(env.arguments.get(0).or(''))\n",
    );
    source.push_str("    tests.run\n");
    source.push_str("  }\n");
    source.push_str("}\n");
    source
}
