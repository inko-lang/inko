use crate::error::Error;
use crate::options::print_usage;
use getopts::Options;
use git2::Repository;
use std::path::{Path, PathBuf};

const USAGE: &str = "Usage: inko install [PACKAGE_SPEC]

Installs an Inko package from a git repository.

This currently makes a few assumptions, one being that all source for
a package will be kept in a top-level `src` directory. It also only
accepts \"host==version\" package specifications.

Example:

    inko install \"gitlab.com/inko-lang/http==1.0.0\"";

const PACKAGES_PATH: &str = "inko/packages";

/// Installs an Inko package
pub fn install(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Shows this help message");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    if let Some(package_spec) = matches.free.get(0) {
        install_package(package_spec)
    } else {
        Err(Error::generic(
            "You must specify a package to install".to_string(),
        ))
    }
}

fn install_package(package_spec: &str) -> Result<i32, Error> {
    // make sure our data & cache directories are present
    let (cache_dir, install_dir) = ensure_dirs()?;

    // parse "host==version" into <host> <op> <version>
    let (host, _op, version) = split_spec(package_spec).ok_or_else(|| {
        Error::generic(format!("error parsing package spec '{}'", package_spec))
    })?;

    let repo_path = cache_dir.join(host);

    let repo = if repo_path.exists() {
        // update repository in-place
        println!("updating cache for {}", host);
        update(&repo_path)?
    } else {
        // clone repo into $CACHE/<host>
        println!("cloning {}", host);
        clone(host, &repo_path)?
    };

    // keep a reference to the original HEAD
    let head = repo.head()?;
    let head = head
        .target()
        .ok_or_else(|| Error::generic("couldn't get the name of HEAD"))?;
    let head = repo.find_object(head, None)?;

    let reference = repo.resolve_reference_from_short_name(&version)?;
    let refname = reference.target().ok_or_else(|| {
        Error::generic(format!("couldn't get name for ref '{}'", version))
    })?;
    let obj = repo.find_object(refname, None)?;

    // checkout <version> tag
    // TODO: maybe accept tags prefixed with "v" too? so foo==1.0.0 could use tag v1.0.0, for example
    if let Err(e) = repo.checkout_tree(&obj, None) {
        // try to reset cached checkout if something goes wrong
        reset(&repo, &head)?;
        return Err(e.into());
    }

    println!("installing {} version {}", host, version);

    // TODO maybe build and move the bytecode to $DATA/<host>/<version> instead?
    // copy source to $DATA/<host>/<version>
    let install_path = install_dir.join(host).join(version);
    if let Err(e) = dircpy::copy_dir(repo_path, install_path) {
        // TODO this needs to exclude a bunch of stuff
        reset(&repo, &head)?;
        return Err(e.into());
    }

    // reset cache to saved
    reset(&repo, &head)?;
    Ok(0)
}

fn reset(repo: &Repository, to: &git2::Object<'_>) -> Result<(), Error> {
    repo.checkout_tree(to, None)?;
    Ok(())
}

fn ensure_dirs() -> Result<(PathBuf, PathBuf), Error> {
    use std::fs;

    let cache = dirs_next::cache_dir()
        .ok_or_else(|| Error::generic("No cache dir found"))?;
    let cache = cache.join(PACKAGES_PATH);
    let data = dirs_next::data_dir()
        .ok_or_else(|| Error::generic("No data dir found"))?;
    let data = data.join(PACKAGES_PATH);
    if !cache.exists() {
        fs::create_dir_all(&cache)?;
    }
    if !data.exists() {
        fs::create_dir_all(&data)?;
    }

    Ok((cache, data))
}

fn split_spec(spec: &str) -> Option<(&str, &str, &str)> {
    // TODO more operators than just `==`
    let mut parts = spec.splitn(2, "==");
    let host = parts.next()?;
    let version = parts.next()?;
    Some((host, "==", version))
}

fn update(to: &Path) -> Result<Repository, Error> {
    let repo = Repository::open(to)?;
    {
        // repo.find_remote borrows repo and prevents returning it at the
        // end of this function if we don't introduce a new scope
        let mut remote = repo.find_remote("origin")?;
        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.download_tags(git2::AutotagOption::All);
        remote.fetch(
            &["refs/heads/*:refs/heads/*"],
            Some(&mut fetch_opts),
            None,
        )?;
        let head = repo
            .head()?
            .target()
            .ok_or_else(|| Error::generic("couldn't get target for HEAD"))?;
        let obj = repo.find_object(head, None)?;
        repo.reset(&obj, git2::ResetType::Hard, None)?;
    }
    Ok(repo)
}

fn clone(host: &str, to: &Path) -> Result<Repository, Error> {
    let url = format!("https://{}", host);
    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.download_tags(git2::AutotagOption::All);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_opts);

    Ok(builder.clone(&url, &to)?)
}
