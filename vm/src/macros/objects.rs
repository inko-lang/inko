#![macro_use]

macro_rules! copy_if_permanent {
    ($heap: expr, $source: expr, $dest: expr) => ({
        if $dest.is_permanent() {
            $heap.lock().copy_object($source)
        }
        else {
            $source
        }
    });
}

/// Returns true if a given pointer is false.
macro_rules! is_false {
    ($machine: expr, $pointer: expr) => (
        $pointer == $machine.state.false_object ||
            $pointer == $machine.state.nil_object
    )
}
