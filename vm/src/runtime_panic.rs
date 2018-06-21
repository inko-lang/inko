use colored::*;
use process::RcProcess;

/// Prints a runtime panic to STDERR.
pub fn display_panic(process: &RcProcess, message: &str) {
    let mut frames = Vec::new();

    for context in process.context().contexts() {
        frames.push(format!(
            "{}, line {}, in {}",
            format!("{:?}", context.code.file).green(),
            context.line.to_string().cyan(),
            format!("{:?}", context.code.name).yellow()
        ));
    }

    frames.reverse();

    eprintln!("Stack trace (the most recent call comes last):");

    let index_padding = frames.len().to_string().len();

    for (index, line) in frames.iter().enumerate() {
        eprintln!(
            "  {}: {}",
            format!("{:01$}", index, index_padding).cyan(),
            line
        );
    }

    eprintln!(
        "Process {} panicked: {}",
        process.pid.to_string().cyan(),
        message.bold()
    );
}
