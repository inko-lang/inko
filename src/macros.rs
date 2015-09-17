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
