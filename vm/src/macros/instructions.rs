#![macro_use]

/// Calls an instruction method on a given receiver.
macro_rules! run {
    ($rec: expr, $name: ident, $process: ident, $code: ident, $ins: ident) => (
        try!($rec.$name($process.clone(), $code.clone(), &$ins))
    );
}
