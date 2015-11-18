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

/// Returns an Err if a given prototype has already been defined.
macro_rules! error_when_prototype_exists {
    ($rec: expr, $name: ident) => (
        if read_lock!($rec.memory_manager).$name().is_some() {
            return Err("prototype already defined".to_string());
        }
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

/// Returns an RcObject from a thread using an instruction argument.
macro_rules! instruction_object {
    ($ins: ident, $thread: ident, $index: expr) => ({
        let index = try!($ins.arg($index));

        try!($thread.get_register(index))
    });
}
