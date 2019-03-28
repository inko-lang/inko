use process::RcProcess;

/// Prints a runtime panic to STDERR.
pub fn display_panic(process: &RcProcess, message: &str) {
    let mut frames = Vec::new();

    for context in process.context().contexts() {
        frames.push(format!(
            "{}, line {}, in {}",
            format!("{:?}", context.code.file.string_value().unwrap()),
            context.line.to_string(),
            format!("{:?}", context.code.name.string_value().unwrap())
        ));
    }

    frames.reverse();

    eprintln!("Stack trace (the most recent call comes last):");

    let index_padding = frames.len().to_string().len();

    for (index, line) in frames.iter().enumerate() {
        eprintln!("  {}: {}", format!("{:01$}", index, index_padding), line);
    }

    eprintln!("Process {:#x} panicked: {}", process.identifier(), message);
}
