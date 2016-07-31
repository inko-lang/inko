#![macro_use]

macro_rules! copy_if_permanent {
    ($heap: expr, $source: expr, $dest: expr) => ({
        if $dest.is_permanent() {
            let (copy, _) = write_lock!($heap).copy_object($source);

            copy
        }
        else {
            $source
        }
    });
}
