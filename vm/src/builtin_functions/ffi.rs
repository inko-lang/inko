//! Functions for interacting with C code from Inko.
use crate::ffi::{
    type_alignment, type_size, Function, Library, Pointer as ForeignPointer,
};
use crate::mem::{Array, Int, Pointer, String as InkoString};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;

pub(crate) fn ffi_library_open(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let names = unsafe { arguments[0].get::<Array>() }.value();
    let result = Library::from_pointers(names)
        .map(Pointer::boxed)
        .unwrap_or_else(Pointer::undefined_singleton);

    Ok(result)
}

pub(crate) fn ffi_function_attach(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let func = unsafe {
        let lib = arguments[0].get::<Library>();
        let name = InkoString::read(&arguments[1]);
        let args = arguments[2].get::<Array>().value();
        let rtype = arguments[3];

        Function::attach(lib, name, args, rtype)?
    };

    Ok(func.map(Pointer::boxed).unwrap_or_else(Pointer::undefined_singleton))
}

pub(crate) fn ffi_function_call(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let func = unsafe { arguments[0].get::<Function>() };
    let args = &arguments[1..];

    Ok(unsafe { func.call(state, args)? })
}

pub(crate) fn ffi_pointer_attach(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let lib = unsafe { arguments[0].get::<Library>() };
    let name = unsafe { InkoString::read(&arguments[1]) };
    let raw_ptr = unsafe {
        lib.get(name)
            .map(|ptr| Pointer::new(ptr.as_ptr()))
            .unwrap_or_else(Pointer::undefined_singleton)
    };

    Ok(raw_ptr)
}

pub(crate) fn ffi_pointer_read(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let ptr = ForeignPointer::new(arguments[0].as_ptr() as _);
    let kind = arguments[1];
    let offset = unsafe { Int::read(arguments[2]) as usize };
    let result = unsafe { ptr.with_offset(offset).read_as(state, kind)? };

    Ok(result)
}

pub(crate) fn ffi_pointer_write(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let ptr = ForeignPointer::new(arguments[0].as_ptr() as _);
    let kind = arguments[1];
    let offset = unsafe { Int::read(arguments[2]) as usize };
    let value = arguments[3];

    unsafe {
        ptr.with_offset(offset).write_as(kind, value)?;
    }

    Ok(Pointer::nil_singleton())
}

pub(crate) fn ffi_pointer_from_address(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let addr = unsafe { Int::read(arguments[0]) };

    Ok(Pointer::new(addr as _))
}

pub(crate) fn ffi_pointer_address(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let addr = arguments[0].as_ptr() as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), addr))
}

pub(crate) fn ffi_type_size(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { Int::read(arguments[0]) };

    type_size(kind).map_err(|e| e.into())
}

pub(crate) fn ffi_type_alignment(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { Int::read(arguments[0]) };

    type_alignment(kind).map_err(|e| e.into())
}

pub(crate) fn ffi_library_drop(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<Library>();
    }

    Ok(Pointer::nil_singleton())
}

pub(crate) fn ffi_function_drop(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<Function>();
    }

    Ok(Pointer::nil_singleton())
}
