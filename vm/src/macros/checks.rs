#![macro_use]

/// Returns an Err if any of the given arguments is not an integer.
macro_rules! ensure_integers {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_integer() {
                return Err("all arguments must be Integer objects".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not a float.
macro_rules! ensure_floats {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_float() {
                return Err("all arguments must be Float objects".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not an array.
macro_rules! ensure_arrays {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_array() {
                return Err("all arguments must be Array objects".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not a string.
macro_rules! ensure_strings {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_string() {
                return Err("all arguments must be String objects".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not a file.
macro_rules! ensure_files {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_file() {
                return Err("all arguments must be File objects".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not a CompiledCode value.
macro_rules! ensure_compiled_code {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_compiled_code() {
                return Err("all arguments must be CompiledCode objects".to_string());
            }
        )+
    );
}

/// Ensures the given index is within the bounds of the array.
macro_rules! ensure_array_within_bounds {
    ($array: ident, $index: expr) => (
        if $index >= $array.len() {
            return Err(format!("index {} is out of bounds", $index));
        }
    );
}

/// Ensures the given number of bytes to read is greater than 0
macro_rules! ensure_positive_read_size {
    ($size: expr) => (
        if $size < 0 {
            return Err("can't read a negative amount of bytes".to_string());
        }
    );
}
