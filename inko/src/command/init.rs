use crate::error::Error;
use crate::options::print_usage;
use compiler::config::SOURCE_EXT;
use compiler::pkg::manifest::{MANIFEST_FILE, Manifest};
use compiler::pkg::version::Version;
use getopts::Options;
use std::env;
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
    inko init .           # Create a project in the current working directory
    inko init hello --lib # Create a project that's an Inko library";

const BIN: &str = include_str!("../../templates/main.inko");
const GITIGNORE: &str = include_str!("../../templates/gitignore");
const GITHUB_WORKFLOW: &str = include_str!("../../templates/github.yml");
const CODEBERG_WORKFLOW: &str = include_str!("../../templates/forgejo.yml");
const LICENSE: &str = include_str!("../../templates/LICENSE");

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");
    options.optflag("", "lib", "Create a new library");
    options.optflag("", "github", "Generate CI configuration for GitHub");
    options.optflag("", "codeberg", "Generate CI configuration for Codeberg");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let Some(mut name) = matches.free.first().cloned() else {
        return Err(Error::from("a project name is required".to_string()));
    };

    let root = if name == "." {
        let pwd = env::current_dir().map_err(|e| {
            format!("failed to determine the current working directory: {}", e)
        })?;

        if let Some(v) = pwd.file_name() {
            name = v.to_string_lossy().into_owned();
        } else {
            return Err(Error::from(
                "failed to derive the project name \
                from the current working directory"
                    .to_string(),
            ));
        }

        pwd
    } else {
        let dir = PathBuf::from(&name);

        if dir.is_dir() {
            return Err(Error::from(format!(
                "the directory '{}' already exists",
                name
            )));
        }

        dir
    };

    let bin = !matches.opt_present("lib");
    let src = root.join("src");
    let test = root.join("test");
    let main_name = name.strip_prefix("inko-").unwrap_or(&name);
    let mut main = src.join(main_name);

    main.set_extension(SOURCE_EXT);

    create_dir_all(&src)?;
    create_dir(&test)?;
    create_file(&test.join(".gitkeep"), "")?;
    create_file(&main, if bin { BIN } else { "" })?;
    create_file(&root.join(".gitignore"), GITIGNORE)?;
    create_file(&root.join("LICENSE"), LICENSE)?;

    let mut manifest = Manifest::new();

    manifest.set_inko_version(Version::inko());
    manifest.save(&root.join(MANIFEST_FILE))?;

    if matches.opt_present("github") {
        let dir = root.join(".github").join("workflows");

        create_dir_all(&dir)?;
        create_file(&dir.join("push.yml"), GITHUB_WORKFLOW)?;
    }

    if matches.opt_present("codeberg") {
        let dir = root.join(".forgejo").join("workflows");

        create_dir_all(&dir)?;
        create_file(&dir.join("push.yml"), CODEBERG_WORKFLOW)?;
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
