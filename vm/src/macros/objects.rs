#![macro_use]

macro_rules! copy_if_global {
    ($heap: expr, $source: expr, $dest: expr) => ({
        if $dest.is_global() {
            write_lock!($heap).copy_object($source)
        }
        else {
            $source
        }
    });
}
