use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::{Config, Linker, Opt, SOURCE};
use compiler::diagnostics::info;
use getopts::Options;
use std::path::PathBuf;
use types::module_name::ModuleName;

const USAGE: &str = "Usage: inko build [OPTIONS] [FILE]

Compile a source file and its dependencies into an executable.

Examples:

    inko build             # Compile all executables for the project
    inko build hello.inko  # Compile the file hello.inko";

enum Timings {
    None,
    Basic,
    Full,
}

impl Timings {
    fn parse(value: &str) -> Option<Timings> {
        match value {
            "basic" => Some(Timings::Basic),
            "full" => Some(Timings::Full),
            _ => None,
        }
    }
}

fn parse_compile_time_variable(
    value: &str,
) -> Option<(ModuleName, String, String)> {
    let (key, val) = value.split_once('=')?;
    let mut path = key.split('.');
    let name = match path.next_back() {
        Some(v) if !v.is_empty() => v,
        _ => return None,
    };

    let mut module_name = String::new();

    for step in path {
        if !module_name.is_empty() {
            module_name.push('.');
        }

        module_name.push_str(step);
    }

    if module_name.is_empty() {
        return None;
    }

    Some((ModuleName::new(module_name), name.to_string(), val.to_string()))
}

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");
    options.optopt(
        "f",
        "format",
        "The output format to use for diagnostics",
        "FORMAT",
    );
    options.optopt(
        "t",
        "target",
        "The target platform to compile for",
        "TARGET",
    );
    options.optmulti(
        "i",
        "include",
        "A directory to add to the list of source directories",
        "PATH",
    );
    options.optflag("", "release", "Perform a release build");
    options.optflag("", "static", "Statically link imported C libraries");
    options.optflag("", "dot", "Output the MIR of every module as DOT files");
    options.optflag("", "mir", "Output the MIR of every module as text files");
    options.optflag("", "verify", "Verify build output at various stages");
    options.optflag("", "write-llvm", "Write LLVM IR files to disk");
    options.optflagopt(
        "",
        "timings",
        "Display the time spent compiling code",
        "basic,full",
    );
    options.optopt(
        "",
        "threads",
        "The number of threads to use for parallel compilation",
        "NUM",
    );
    options.optopt(
        "",
        "linker",
        "A custom linker to use, instead of detecting the linker automatically",
        "LINKER",
    );
    options.optmulti(
        "",
        "linker-arg",
        "An extra argument to pass to the linker",
        "ARG",
    );
    options.optflag(
        "",
        "disable-incremental",
        "Disables incremental compilation",
    );
    options.optmulti(
        "d",
        "define",
        "Define a custom value for a public constant",
        "NAME=VALUE",
    );

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut config = Config::default();

    if let Some(val) = matches.opt_str("format") {
        config.set_presenter(&val)?;
    }

    if let Some(val) = matches.opt_str("target") {
        config.set_target(&val)?;
    }

    if matches.opt_present("release") {
        config.opt = Opt::Release;
    }

    if matches.opt_present("dot") {
        config.write_dot = true;
    }

    if matches.opt_present("mir") {
        config.write_mir = true;
    }

    if matches.opt_present("verify") {
        config.verify = true;
    }

    if matches.opt_present("write-llvm") {
        config.write_llvm = true;
    }

    if matches.opt_present("static") {
        config.static_linking = true;
    }

    for path in matches.opt_strs("i") {
        config.add_source_directory(path.into());
    }

    if matches.opt_present("disable-incremental") {
        config.incremental = false;
    }

    if let Some(val) = matches.opt_str("threads") {
        match val.parse::<usize>() {
            Ok(0) | Err(_) => {
                return Err(Error::from(format!(
                    "'{}' isn't a valid number of threads",
                    val
                )));
            }
            Ok(n) => config.threads = n,
        };
    }

    if let Some(val) = matches.opt_str("linker") {
        config.linker = Linker::parse(&val).ok_or_else(|| {
            Error::from(format!("'{}' isn't a valid linker", val))
        })?;
    }

    for arg in matches.opt_strs("linker-arg") {
        config.linker_arguments.push(arg);
    }

    let timings = match matches.opt_str("timings") {
        Some(val) => Timings::parse(&val).ok_or_else(|| {
            Error::from(format!("'{}' is an invalid --timings argument", val))
        })?,
        _ if matches.opt_present("timings") => Timings::Basic,
        _ => Timings::None,
    };

    for val in matches.opt_strs("define") {
        if let Some((module, name, val)) = parse_compile_time_variable(&val) {
            config.compile_time_variables.insert((module, name), val);
        } else {
            return Err(Error::from(format!(
                "the --define='{}' option is invalid, \
                values must be in the format 'a.b.c.CONSTANT=VALUE'",
                val,
            )));
        }
    }

    let inputs = if let Some(v) = matches.free.first() {
        vec![PathBuf::from(v)]
    } else {
        match config.executable_sources() {
            Ok(paths) if paths.is_empty() => {
                return Err(Error::from(format!(
                    "to build an executable the {}/ directory must contain at \
                    least one source file (e.g. {}/example.inko)",
                    SOURCE, SOURCE
                )));
            }
            Ok(paths) => paths,
            Err(e) => {
                return Err(Error::from(format!(
                    "failed to get the files to compile: {}",
                    e
                )))
            }
        }
    };
    let multiple = inputs.len() > 1;
    let mut status = 0;

    for path in inputs {
        // If there's only a single executable to build then there's no point in
        // showing which one we're building.
        if multiple {
            let rel = path.strip_prefix(&config.source).unwrap();

            // If the previous executable failed to build we add an extra empty
            // line so it's a little easier to read the output from the
            // different builds.
            if status == 1 {
                println!();
            }

            info("Compiling", &rel.display().to_string());
        }

        let mut compiler = Compiler::new(config);
        let result = compiler.build(path);

        compiler.print_diagnostics();

        match timings {
            Timings::Basic => compiler.print_timings(),
            Timings::Full => compiler.print_full_timings(),
            _ => {}
        }

        match result {
            Ok(_) => {}
            Err(CompileError::Invalid) => status = 1,
            Err(CompileError::Internal(msg)) => return Err(Error::from(msg)),
        }

        config = compiler.into_config();
    }

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_compile_time_variable() {
        assert_eq!(
            parse_compile_time_variable("a.b.C=D"),
            Some((ModuleName::new("a.b"), "C".to_string(), "D".to_string()))
        );
        assert_eq!(
            parse_compile_time_variable("a.b=D"),
            Some((ModuleName::new("a"), "b".to_string(), "D".to_string()))
        );
        assert_eq!(
            parse_compile_time_variable("a.b.C="),
            Some((ModuleName::new("a.b"), "C".to_string(), String::new()))
        );
        assert_eq!(parse_compile_time_variable("C=D"), None);
        assert_eq!(parse_compile_time_variable("a.b.=D"), None);
        assert_eq!(parse_compile_time_variable(""), None);
        assert_eq!(parse_compile_time_variable("a"), None);
        assert_eq!(parse_compile_time_variable("a.b"), None);
    }
}
