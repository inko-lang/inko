use std::fs::OpenOptions;

/// Builds an fs::OpenOptions based on a fopen() compatible string slice.
pub fn from_fopen_string(input: &str) -> Result<OpenOptions, String> {
    let mut open_opts = OpenOptions::new();

    match input {
        "r"  => { open_opts.read(true); },
        "r+" => {
            open_opts.read(true).write(true).truncate(true).create(true);
        },
        "w"  => { open_opts.write(true).truncate(true).create(true); },
        "w+" => {
            open_opts.read(true).write(true).truncate(true).create(true);
        },
        "a"  => { open_opts.append(true).create(true); },
        "a+" => { open_opts.read(true).append(true).create(true); },
        _    => return Err(format!("unsupported file mode {}", input))
    }

    Ok(open_opts)
}
