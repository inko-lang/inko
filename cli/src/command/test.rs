//! Command for running unit tests.
use crate::command::run;
use crate::config::{MODULE_SEPARATOR, SOURCE_FILE_EXT};
use crate::error::Error;
use crate::options::print_usage;
use getopts::Options;
use std::env;
use std::path::{PathBuf, MAIN_SEPARATOR};

const USAGE: &str = "Usage: inko test [OPTIONS]

Runs unit tests in a directory.

Unit tests are Inko source files that start with the `test_` prefix. This
command will gather all such files and run them. This command expects tests to
be located in a test/ directory, inside a tests/ direcory known as the tests
root. This tests root directory can be changed using the -d/--directory option,
provided the directory contains a test/ directory.

Examples:

    inko test           # Runs all unit tests in ./tests/test
    inko test -d foo    # Runs all unit tests in ./foo/test";

/// The default root directory (relative to the current working directory) for
/// running unit tests.
const DEFAULT_TESTS_ROOT: &str = "tests";

/// The name of the directory that contains unit tests.
const TEST_DIRECTORY: &str = "test";

/// Compiles and runs Inko unit tests.
pub fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Shows this help message");
    options.optopt(
        "d",
        "directory",
        "The path to the directory containing all unit tests",
        "DIR",
    );

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let cwd = env::current_dir()?;
    let root_dir = if let Some(path) = matches.opt_str("d") {
        cwd.join(path)
    } else {
        cwd.join(DEFAULT_TESTS_ROOT)
    };

    let test_dir = root_dir.join(TEST_DIRECTORY);

    if !test_dir.is_dir() {
        return Err(Error::generic(format!(
            "The directory `{}` must contain a `{}` directory",
            root_dir.to_string_lossy(),
            TEST_DIRECTORY
        )));
    }

    let modules = test_modules(&root_dir)?;
    let source = generate_source(modules);

    run::run_eval(
        &source,
        vec![root_dir.to_string_lossy().to_string()],
        &matches.free,
    )
}

/// Returns the full module names of all unit tests.
fn test_modules(directory: &PathBuf) -> Result<Vec<String>, Error> {
    let mut mods = Vec::new();
    let mut dirs = vec![directory.clone()];

    while let Some(dir) = dirs.pop() {
        let contents = dir.read_dir()?;

        for entry_res in contents {
            let entry = entry_res?;
            let ftype = entry.file_type()?;

            if ftype.is_dir() {
                dirs.push(entry.path());
                continue;
            }

            if !ftype.is_file() {
                // Symbolic links are not supported, at least for the time
                // being.
                continue;
            }

            let path = entry.path();

            if !path
                .file_name()
                .map(|p| p.to_string_lossy().starts_with("test_"))
                .unwrap_or(false)
            {
                continue;
            }

            if !path
                .extension()
                .map(|e| e == SOURCE_FILE_EXT)
                .unwrap_or(false)
            {
                continue;
            }

            let mut mod_path = path
                .strip_prefix(directory)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace(MAIN_SEPARATOR, MODULE_SEPARATOR);

            // This removes the trailing source extension, without the need
            // for allocating another String.
            mod_path.truncate(mod_path.len() - (SOURCE_FILE_EXT.len() + 1));

            mods.push(mod_path);
        }
    }

    Ok(mods)
}

/// Generates Inko source code used for running unit tests.
///
/// The resulting source code will look along the lines of the following:
///
///     import std::test
///     import test::foo::bar::(self as _)
///     test.run
fn generate_source(modules: Vec<String>) -> String {
    let mut output = "import std::test\n".to_string();

    for module in modules {
        output.push_str("import ");
        output.push_str(&module);

        // To prevent imported module names from conflicting, we import them as
        // `_`; resulting in no symbols being created for the imported module.
        output.push_str(MODULE_SEPARATOR);
        output.push_str("(self as _)\n");
    }

    output.push_str("test.run");
    output
}
