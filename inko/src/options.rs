//! Generic helper functions that don't belong to any particular module.
use getopts::Options;

/// Prints a usage message for a set of CLI options.
pub(crate) fn print_usage(options: &Options, brief: &str) {
    let out = options.usage_with_format(|opts| {
        format!(
            "{}\n\nOptions:\n\n{}",
            brief,
            opts.collect::<Vec<String>>().join("\n")
        )
    });

    println!("{}", out);
}
