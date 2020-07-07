//! VM functions for working with the Foreign Function Interface.
use crate::ffi;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

#[inline(always)]
pub fn ffi_library_open(
    process: &RcProcess,
    names_ptr: ObjectPointer,
    proto_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let names = names_ptr.array_value()?;
    let lib = ffi::Library::from_pointers(names)?;

    Ok(process.allocate(object_value::library(lib), proto_ptr))
}

#[inline(always)]
pub fn ffi_function_attach(
    process: &RcProcess,
    lib: ObjectPointer,
    name: ObjectPointer,
    arg_types: ObjectPointer,
    rtype: ObjectPointer,
    proto: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let func = unsafe {
        let lib = lib.library_value()?;
        let name = name.string_value()?.as_slice();
        let args = arg_types.array_value()?;

        ffi::Function::attach(lib, name, args, rtype)?
    };

    let result = process.allocate(object_value::function(func), proto);

    Ok(result)
}

#[inline(always)]
pub fn ffi_function_call(
    state: &RcState,
    process: &RcProcess,
    func_ptr: ObjectPointer,
    args_ptr: ObjectPointer,
    pointer_proto_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let func = func_ptr.function_value()?;
    let args = args_ptr.array_value()?;

    Ok(unsafe { func.call(&state, &process, pointer_proto_ptr, args)? })
}

#[inline(always)]
pub fn ffi_pointer_attach(
    process: &RcProcess,
    lib: ObjectPointer,
    name: ObjectPointer,
    proto: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let raw_ptr =
        unsafe { lib.library_value()?.get(name.string_value()?.as_slice())? };

    let result = process.allocate(object_value::pointer(raw_ptr), proto);

    Ok(result)
}

#[inline(always)]
pub fn ffi_pointer_read(
    state: &RcState,
    process: &RcProcess,
    pointer_proto_ptr: ObjectPointer,
    ptr: ObjectPointer,
    read_as: ObjectPointer,
    offset_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let offset = offset_ptr.usize_value()?;

    let result = unsafe {
        ptr.pointer_value()?.with_offset(offset).read_as(
            &state,
            process,
            pointer_proto_ptr,
            read_as,
        )?
    };

    Ok(result)
}

#[inline(always)]
pub fn ffi_pointer_write(
    ptr: ObjectPointer,
    write_as: ObjectPointer,
    value: ObjectPointer,
    offset_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let offset = offset_ptr.usize_value()?;

    unsafe {
        ptr.pointer_value()?
            .with_offset(offset)
            .write_as(write_as, value)?;
    }

    Ok(value)
}

#[inline(always)]
pub fn ffi_pointer_from_address(
    process: &RcProcess,
    addr: ObjectPointer,
    proto: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = process.allocate(
        object_value::pointer(unsafe { ffi::Pointer::from_address(addr)? }),
        proto,
    );

    Ok(result)
}

#[inline(always)]
pub fn ffi_pointer_address(
    state: &RcState,
    process: &RcProcess,
    ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = process.allocate_usize(
        ptr.pointer_value()?.address(),
        state.integer_prototype,
    );

    Ok(result)
}

#[inline(always)]
pub fn ffi_type_size(kind: ObjectPointer) -> Result<ObjectPointer, String> {
    ffi::type_size(kind.integer_value()?)
}

#[inline(always)]
pub fn ffi_type_alignment(
    kind: ObjectPointer,
) -> Result<ObjectPointer, String> {
    ffi::type_alignment(kind.integer_value()?)
}
