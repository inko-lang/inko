use crate::error::Error;
use crate::options::print_usage;
use compiler::config::SOURCE_EXT;
use compiler::pkg::manifest::{Manifest, MANIFEST_FILE};
use compiler::pkg::version::Version;
use getopts::Options;
use std::fs;
use std::path::{Path, PathBuf};

const USAGE: &str = "Usage: inko init [OPTIONS] [NAME]

Create a new project in the current working directory.

By default an executable project is created. To create a library instead, use
the --lib option.

If the name of the project starts with \"inko-\", the prefix is removed from the
source file stored in the src/ directory.

Examples:

    inko init hello       # Create a project that compiles an executable
    inko init inko-hello  # Creates ./inko-hello containing a src/hello.inko file
    inko init hello --lib # Create a project that's an Inko library";

const BIN: &str = "type async Main {
  fn async main {}
}
";

const GITIGNORE: &str = "\
/build
/dep
";

const GITHUB_WORKFLOW: &str = "\
---
name: Push
on:
  push:
  pull_request:

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

jobs:
  fmt:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/inko-lang/inko:latest
    steps:
      - uses: actions/checkout@v4
      - run: inko --version
      - run: inko fmt --check

  test:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/inko-lang/inko:latest
    steps:
      - uses: actions/checkout@v4
      - run: inko --version
      - run: inko test
";

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");
    options.optflag("", "lib", "Create a new library");
    options.optflag("", "github", "Create a basic workflow for GitHub Actions");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let Some(name) = matches.free.first() else {
        return Err(Error::from("a project name is required".to_string()));
    };

    let root = PathBuf::from(name);

    if root.is_dir() {
        return Err(Error::from(format!(
            "the directory '{}' already exists",
            name
        )));
    }

    let bin = !matches.opt_present("lib");
    let src = root.join("src");
    let test = root.join("test");
    let main_name = name.strip_prefix("inko-").unwrap_or(name);
    let mut main = src.join(main_name);

    main.set_extension(SOURCE_EXT);

    create_dir_all(&src)?;
    create_dir(&test)?;
    create_file(&test.join(".gitkeep"), "")?;
    create_file(&main, if bin { BIN } else { "" })?;
    create_file(&root.join(".gitignore"), GITIGNORE)?;

    let mut manifest = Manifest::new();

    manifest.set_inko_version(Version::inko());
    manifest.save(&root.join(MANIFEST_FILE))?;

    if matches.opt_present("github") {
        let dir = root.join(".github").join("workflows");

        create_dir_all(&dir)?;
        create_file(&dir.join("push.yml"), GITHUB_WORKFLOW)?;
    }

    Ok(0)
}

fn create_dir(path: &Path) -> Result<(), Error> {
    fs::create_dir(path).map_err(|e| {
        Error::from(format!("failed to create '{}': {}", path.display(), e))
    })
}

fn create_dir_all(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path).map_err(|e| {
        Error::from(format!("failed to create '{}': {}", path.display(), e))
    })
}

fn create_file(path: &Path, body: &str) -> Result<(), Error> {
    fs::write(path, body).map_err(|e| {
        Error::from(format!("failed to create '{}': {}", path.display(), e))
    })
}
