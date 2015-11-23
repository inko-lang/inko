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

/// Returns a new, empty and pinned object.
macro_rules! empty_pinned_object {
    ($id: expr) => ({
        let object = Object::new($id, object_value::none());

        write_lock!(object).pin();

        object
    });
}

/// Maps an Err to Result<(), String> using the Err's description
macro_rules! map_error {
    ($expr: expr) => (
        $expr.map_err(|err| { err.description().to_string() })
    );
}
