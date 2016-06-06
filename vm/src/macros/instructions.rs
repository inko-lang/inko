#![macro_use]

/// Calls an instruction method on a given receiver.
macro_rules! run {
    ($rec: expr, $name: ident, $process: ident, $code: ident, $ins: ident) => (
        try!($rec.$name($process.clone(), $code.clone(), &$ins));
    );
}

/// Returns an RcObject from a thread using an instruction argument.
macro_rules! instruction_object {
    ($ins: expr, $process: expr, $index: expr) => ({
        let index = try!($ins.arg($index));
        let lock = read_lock!($process);

        try!(lock.get_register(index))
    });
}
