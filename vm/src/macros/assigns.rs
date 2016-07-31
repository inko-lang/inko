#![macro_use]

macro_rules! reassign_if_true {
    ($target: expr, $check: expr) => ({
        if $check {
            $target = $check;
        }
    });
}
