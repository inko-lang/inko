#![macro_use]

/// Sets an error in a register and returns control to the caller.
macro_rules! set_error {
    ($code: expr, $process: expr, $register: expr) => ({
        let mut lock = write_lock!($process);
        let obj = lock.allocate_without_prototype(object_value::error($code));

        lock.set_register($register, obj);

        return Ok(());
    });
}

macro_rules! return_vm_error {
    ($message: expr, $line: expr) => (
        return Err(VirtualMachineError::new($message, $line))
    )
}

macro_rules! try_vm_error {
    ($expr: expr, $ins: expr) => (
        try!($expr.map_err(|err| VirtualMachineError::new(err, $ins.line)));
    );
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
        try_error!($expr.map_err(|err| errors::from_io_error(err)), $process,
                   $register)
    );
}

/// Tries to create a String from a collection of bytes.
macro_rules! try_from_utf8 {
    ($bytes: expr) => (
        String::from_utf8($bytes).map_err(|_| errors::STRING_INVALID_UTF8 )
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
