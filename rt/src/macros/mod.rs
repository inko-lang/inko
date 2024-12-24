#![macro_use]

/// A macro for initialising a struct field, without dropping the existing
/// value.
///
/// We use this macro instead of `forget(replace(x, y))`, as that pattern
/// produces more instructions than just a regular field write.
///
/// This macro exists as various objects require manually initialising fields. A
/// regular field assignment would drop the existing (uninitialised) field
/// value, resulting in a crash.
///
/// # Examples
///
///     init!(some_object.header.instance_of => type);
macro_rules! init {
    ($field: expr => $value: expr) => {
        #[allow(unused_unsafe)]
        unsafe {
            std::ptr::addr_of_mut!($field).write($value);
        }
    };
}

#[cfg(target_os = "macos")]
macro_rules! asm_func {
    ($name: expr, $($body: tt)*) => {
        std::arch::global_asm!(concat!(
            ".global _", $name, "\n",
            "_", $name, ":\n",
            $($body)*
        ));
    }
}

#[cfg(not(target_os = "macos"))]
macro_rules! asm_func {
    ($name: expr, $($body: tt)*) => {
        std::arch::global_asm!(concat!(
            ".global ", $name, "\n",
            $name, ":\n",
            $($body)*
        ));
    }
}
