#![macro_use]

/// Acquires a read lock from an RwLock.
macro_rules! read_lock {
    ($value: expr) => (
        $value.read().unwrap()
    );
}

/// Acquires a write lock from an RwLock
macro_rules! write_lock {
    ($value: expr) => (
        $value.write().unwrap()
    );
}

/// Calls an instruction method on a given receiver.
macro_rules! run {
    ($rec: expr, $name: ident, $thread: ident, $code: ident, $ins: ident) => (
        try!($rec.$name($thread.clone(), $code.clone(), &$ins));
    );
}

/// Returns an Err if any of the given arguments is not an integer.
macro_rules! ensure_integers {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_integer() {
                return Err("all objects must be integers".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not a float.
macro_rules! ensure_floats {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_float() {
                return Err("all objects must be floats".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not an array.
macro_rules! ensure_arrays {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_array() {
                return Err("all objects must be arrays".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not a string.
macro_rules! ensure_strings {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_string() {
                return Err("all objects must be strings".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not a file.
macro_rules! ensure_files {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_file() {
                return Err("all objects must be files".to_string());
            }
        )+
    );
}

/// Returns an Err if any of the given arguments is not a CompiledCode value.
macro_rules! ensure_compiled_code {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_compiled_code() {
                return Err("all objects must be compiled code objects".to_string());
            }
        )+
    );
}

/// Returns an RcObject from a thread using an instruction argument.
macro_rules! instruction_object {
    ($ins: ident, $thread: ident, $index: expr) => ({
        let index = try!($ins.arg($index));

        try!($thread.get_register(index))
    });
}

/// Ensures the given index is within the bounds of the array.
macro_rules! ensure_array_within_bounds {
    ($array: ident, $index: expr) => (
        if $index > $array.len() {
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

/// Returns a string to use for reading from a file, optionally with a max size.
macro_rules! file_reading_buffer {
    ($instruction: ident, $thread: ident, $size_idx: expr) => (
        if $instruction.arguments.get($size_idx).is_some() {
            let size_lock = instruction_object!($instruction, $thread,
                                                $size_idx);

            let size_obj = read_lock!(size_lock);

            ensure_integers!(size_obj);

            let size = size_obj.value.as_integer();

            ensure_positive_read_size!(size);

            String::with_capacity(size as usize)
        }
        else {
            String::new()
        }
    );
}

/// Sets an error in a register slot and returns control to the caller.
macro_rules! set_error {
    ($name: expr, $vm: expr, $thread: expr, $slot: expr) => ({
        $thread.set_register($slot, $vm.allocate_error($name));

        return Ok(());
    });
}

/// Returns a Result's OK value or stores the error in a register slot.
macro_rules! try_error {
    ($expr: expr, $vm: expr, $thread: expr, $slot: expr) => (
        match $expr {
            Ok(val)   => val,
            Err(name) => set_error!(name, $vm, $thread, $slot)
        }
    );
}

/// Returns a Result's OK value or stores an IO error in a register slot.
macro_rules! try_io {
    ($expr: expr, $vm: expr, $thread: expr, $slot: expr) => (
        try_error!($expr.map_err(|err| errors::from_io_error(err)),
                   $vm, $thread, $slot)
    );
}

/// Tries to create a String from a collection of bytes.
macro_rules! try_from_utf8 {
    ($bytes: expr) => (
        String::from_utf8($bytes).map_err(|_| errors::STRING_INVALID_UTF8 )
    );
}
