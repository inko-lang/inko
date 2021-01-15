//! Functions for interacting with C code from Inko.
use crate::ffi;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Loads a C library.
///
/// This function requires one argument: an array of library names to use for
/// loading the library.
pub fn ffi_library_open(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let names_ptr = arguments[0];
    let names = names_ptr.array_value()?;
    let lib = ffi::Library::from_pointers(names)
        .map_err(RuntimeError::ErrorMessage)?;

    Ok(process
        .allocate(object_value::library(lib), state.ffi_library_prototype))
}

/// Loads a C function from a library.
///
/// This function requires the following arguments:
///
/// 1. The libraby to load the function from.
/// 2. The name of the function to load.
/// 3. The types of the function arguments.
/// 4. The return type of the function.
pub fn ffi_function_attach(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let func = unsafe {
        let lib = arguments[0].library_value()?;
        let name = arguments[1].string_value()?.as_slice();
        let args = arguments[2].array_value()?;

        ffi::Function::attach(lib, name, args, arguments[3])?
    };

    let result = process
        .allocate(object_value::function(func), state.ffi_function_prototype);

    Ok(result)
}

/// Calls a C function.
///
/// This function requires the following arguments:
///
/// 1. The function to call.
/// 2. An array containing the function arguments.
pub fn ffi_function_call(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let func = arguments[0].function_value()?;
    let args = arguments[1].array_value()?;

    Ok(unsafe { func.call(&state, &process, args)? })
}

/// Loads a C global variable as pointer.
///
/// This function requires the following arguments:
///
/// 1. The library to load the pointer from.
/// 2. The name of the variable.
pub fn ffi_pointer_attach(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let name = arguments[1].string_value()?.as_slice();
    let raw_ptr = unsafe {
        arguments[0]
            .library_value()?
            .get(name)
            .map_err(RuntimeError::ErrorMessage)?
    };

    let result = process
        .allocate(object_value::pointer(raw_ptr), state.ffi_pointer_prototype);

    Ok(result)
}

/// Returns the value of a pointer.
///
/// This function requires the following arguments:
///
/// 1. The pointer to read from.
/// 2. The type to read the data as.
/// 3. The read offset in bytes.
pub fn ffi_pointer_read(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let offset = arguments[2].usize_value()?;

    let result = unsafe {
        arguments[0].pointer_value()?.with_offset(offset).read_as(
            &state,
            process,
            arguments[1],
        )?
    };

    Ok(result)
}

/// Writes a value to a pointer.
///
/// This function requires the following arguments:
///
/// 1. The pointer to write to.
/// 2. The type of data being written.
/// 3. The value to write.
/// 4. The offset to write to.
pub fn ffi_pointer_write(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = arguments[2];
    let offset = arguments[3].usize_value()?;

    unsafe {
        arguments[0]
            .pointer_value()?
            .with_offset(offset)
            .write_as(arguments[1], value)?;
    }

    Ok(value)
}

/// Creates a C pointer from an address.
///
/// This function requires a single argument: the address to use for the
/// pointer.
pub fn ffi_pointer_from_address(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let result = process.allocate(
        object_value::pointer(unsafe {
            ffi::Pointer::from_address(arguments[0])?
        }),
        state.ffi_pointer_prototype,
    );

    Ok(result)
}

/// Returns the address of a pointer.
///
/// This function requires a single argument: the pointer to get the address of.
pub fn ffi_pointer_address(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let result = process.allocate_usize(
        arguments[0].pointer_value()?.address(),
        state.integer_prototype,
    );

    Ok(result)
}

/// Returns the size of an FFI type.
///
/// This function requires a single argument: an integer indicating the FFI
/// type.
pub fn ffi_type_size(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    ffi::type_size(arguments[0].integer_value()?).map_err(|e| e.into())
}

/// Returns the alignment of an FFI type.
///
/// This function requires a single argument: an integer indicating the FFI
/// type.
pub fn ffi_type_alignment(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    ffi::type_alignment(arguments[0].integer_value()?).map_err(|e| e.into())
}

register!(
    ffi_library_open,
    ffi_function_attach,
    ffi_function_call,
    ffi_pointer_attach,
    ffi_pointer_read,
    ffi_pointer_write,
    ffi_pointer_from_address,
    ffi_pointer_address,
    ffi_type_size,
    ffi_type_alignment
);
