use crate::process::RcProcess;

/// Prints a runtime panic to STDERR.
pub fn display_panic(process: &RcProcess, message: &str) {
    let mut frames = Vec::new();
    let mut buffer = String::new();

    for context in process.context().contexts() {
        frames.push(format!(
            "\"{}\" line {}, in \"{}\"",
            context.code.file.string_value().unwrap(),
            context.line.to_string(),
            context.code.name.string_value().unwrap()
        ));
    }

    frames.reverse();

    buffer.push_str("Stack trace (the most recent call comes last):");

    for (index, line) in frames.iter().enumerate() {
        buffer.push_str(&format!("\n  {}: {}", index, line));
    }

    buffer.push_str(&format!(
        "\nProcess {:#x} panicked: {}",
        process.identifier(),
        message
    ));

    eprintln!("{}", buffer);
}
