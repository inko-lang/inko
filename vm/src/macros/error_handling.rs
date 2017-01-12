#![macro_use]

macro_rules! io_error_code {
    ($process: expr, $io_error: expr) => ({
        let code = $crate::errors::io::from_io_error($io_error);

        $process.allocate_without_prototype($crate::object_value::error(code))
    });
}

/// Sets an error in a register and returns control to the caller.
macro_rules! set_error {
    ($code: expr, $process: expr, $register: expr) => ({
        let obj =
            $process.allocate_without_prototype(object_value::error($code));

        $process.set_register($register, obj);

        return Ok(Action::None);
    });
}

macro_rules! return_vm_error {
    ($message: expr, $line: expr) => (
        return Err($message)
    )
}

/// Returns a Result's OK value or stores the error in a register.
macro_rules! try_error {
    ($expr: expr, $process: expr, $register: expr) => (
        match $expr {
            Ok(val)   => val,
            Err(code) => set_error!(code, $process, $register)
        }
    );
}

/// Returns a Result's OK value or stores an IO error in a register.
macro_rules! try_io {
    ($expr: expr, $process: expr, $register: expr) => (
        try_error!($expr.map_err(|err| $crate::errors::io::from_io_error(err)), $process,
                   $register)
    );
}

/// Tries to create a String from a collection of bytes.
macro_rules! try_from_utf8 {
    ($bytes: expr) => (
        String::from_utf8($bytes).map_err(|_| $crate::errors::string::ErrorKind::InvalidUtf8 as u16)
    );
}

macro_rules! constant_error {
    ($reg: expr, $name: expr) => (
        format!(
            "The object in register {} does not define the constant \"{}\"",
            $reg,
            $name
        )
    )
}

macro_rules! attribute_error {
    ($reg: expr, $name: expr) => (
        format!(
            "The object in register {} does not define the attribute \"{}\"",
            $reg,
            $name
        );
    )
}
