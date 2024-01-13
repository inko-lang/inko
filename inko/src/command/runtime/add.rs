use crate::error::Error;
use crate::http;
use crate::options::print_usage;
use compiler::target::Target;
use flate2::read::GzDecoder;
use getopts::{Options, ParsingStyle};
use std::env::temp_dir;
use std::fs::{remove_dir_all, remove_file, File};
use std::io::{stdout, IsTerminal as _};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use tar::Archive;

/// The base URL from which to download the runtime files.
pub(crate) const URL: &str = "https://releases.inko-lang.org";

const USAGE: &str = "inko runtime add [OPTIONS] TARGET

Add a new runtime for a given target.

Examples:

    inko runtime add arm64-linux-gnu";

struct ProgressBar {
    current: usize,
    total: usize,
    last_percentage: usize,
}

impl ProgressBar {
    fn new(total: usize) -> ProgressBar {
        ProgressBar { current: 0, total, last_percentage: 0 }
    }

    fn add(&mut self, amount: usize) {
        self.current += amount;

        let percent =
            (((self.current as f64) / (self.total as f64)) * 100.0) as usize;

        if percent != self.last_percentage {
            let done_mb = (self.current as f64 / 1024.0 / 1024.0) as usize;
            let total_mb = (self.total as f64 / 1024.0 / 1024.0) as usize;

            self.last_percentage = percent;
            print!("\r  {} MiB / {} MiB ({}%)", done_mb, total_mb, percent);

            let _ = stdout().flush();
        }
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        // This ensures that we always produce a new line after the progress
        // line, even in the event of an error, ensuring future output isn't
        // placed on the same line.
        if self.last_percentage > 0 {
            println!();
        }
    }
}

fn download(target: &Target) -> Result<PathBuf, Error> {
    let archive_name = format!("{}.tar.gz", target);
    let url = format!(
        "{}/runtimes/{}/{}",
        URL,
        env!("CARGO_PKG_VERSION"),
        archive_name,
    );

    let response = http::get(&url)?;
    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    // We don't decompress here right away as that prevents us from reporting
    // progress correctly (due to the total read size being different from the
    // Content-Length value).
    let mut reader = response.into_reader();
    let path = temp_dir().join(archive_name);
    let mut file = File::create(&path).map_err(|e| {
        Error::from(format!("failed to open {}: {}", path.display(), e))
    })?;

    let mut run = true;
    let mut progress = ProgressBar::new(total);
    let is_term = stdout().is_terminal();

    while run {
        let mut buff = [0_u8; 8096];
        let result = reader
            .read(&mut buff)
            .and_then(|len| file.write_all(&buff[0..len]).map(|_| len));

        let read = match result {
            Ok(0) => {
                run = false;
                0
            }
            Ok(n) => n,
            Err(err) => {
                return Err(Error::from(format!(
                    "failed to download the runtime: {}",
                    err
                )));
            }
        };

        if is_term {
            progress.add(read)
        }
    }

    Ok(path)
}

fn unpack(path: &Path, into: &Path) -> Result<(), String> {
    let archive = File::open(path).map_err(|e| e.to_string())?;
    let res = Archive::new(GzDecoder::new(archive))
        .entries()
        .and_then(|entries| {
            for entry in entries {
                entry.and_then(|mut entry| entry.unpack_in(into))?;
            }

            Ok(())
        })
        .map_err(|e| format!("failed to unpack the archive: {}", e))
        .and_then(|_| remove_file(path).map_err(|e| e.to_string()))
        .map(|_| ());

    // This ensures that in the event of an error, we don't leave behind a
    // partially decompressed archive in the runtimes directory.
    if res.is_err() && into.is_dir() {
        let _ = remove_dir_all(into);
    }

    res
}

pub(crate) fn run(
    runtimes: PathBuf,
    arguments: &[String],
) -> Result<i32, Error> {
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let target =
        matches.free.first().and_then(|v| Target::parse(v)).ok_or_else(
            || Error::from("a valid target triple is required".to_string()),
        )?;

    if runtimes.join(target.to_string()).is_dir() {
        return Err(Error::from(format!(
            "the runtime for the target '{}' is already installed",
            target
        )));
    }

    println!("Downloading runtime for target '{}'...", target);

    let tmp_path = download(&target)?;

    unpack(&tmp_path, &runtimes).map(|_| 0).map_err(|e| {
        Error::from(format!("failed to decompress the runtime: {}", e))
    })
}
