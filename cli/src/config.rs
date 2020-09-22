/// The default directory containing the runtime source code.
pub const DEFAULT_RUNTIME_LIB: &str = "/usr/lib/inko/runtime";

/// The default path to the compiler executable.
pub const DEFAULT_COMPILER_BIN: &str = "/usr/lib/inko/compiler/bin/inkoc";

/// The default path to the compiler library code.
pub const DEFAULT_COMPILER_LIB: &str = "/usr/lib/inko/compiler/lib";

/// The extension to use for source files.
pub const SOURCE_FILE_EXT: &str = "inko";

/// The extension to use for bytecode images.
pub const BYTECODE_IMAGE_EXT: &str = "ibi";

/// The separator for modules in an import.
pub const MODULE_SEPARATOR: &str = "::";

pub fn runtime_path() -> &'static str {
    option_env!("INKO_RUNTIME_LIB").unwrap_or(DEFAULT_RUNTIME_LIB)
}

pub fn compiler_bin() -> &'static str {
    option_env!("INKO_COMPILER_BIN").unwrap_or(DEFAULT_COMPILER_BIN)
}

pub fn compiler_lib() -> &'static str {
    option_env!("INKO_COMPILER_LIB").unwrap_or(DEFAULT_COMPILER_LIB)
}
