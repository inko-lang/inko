use crate::mem::{
    ByteArray, Float, Int, Pointer as InkoPointer, String as InkoString,
};
use crate::state::State;
use libffi::low::{
    call as ffi_call, ffi_abi_FFI_DEFAULT_ABI as ABI, ffi_cif, ffi_type,
    prep_cif, types, CodePtr, Error as FFIError,
};
use std::convert::Into;
use std::ffi::{CStr, OsStr};
use std::fmt::{Debug, Display};
use std::mem;
use std::os::raw::{c_char, c_double, c_float, c_int, c_long, c_short, c_void};
use std::ptr;

#[repr(i64)]
#[derive(Copy, Clone)]
enum Type {
    /// The numeric identifier of the C `void` type.
    Void,

    /// The numeric identifier of the C `void*` type.
    Pointer,

    /// The numeric identifier of the C `double` type.
    F64,

    /// The numeric identifier of the C `float` type.
    F32,

    /// The numeric identifier of the C `signed char` type.
    I8,

    /// The numeric identifier of the C `short` type.
    I16,

    /// The numeric identifier of the C `int` type.
    I32,

    /// The numeric identifier of the C `long` type.
    I64,

    /// The numeric identifier of the C `unsigned char` type.
    U8,

    /// The numeric identifier of the C `unsigned short` type.
    U16,

    /// The numeric identifier of the C `unsigned int` type.
    U32,

    /// The numeric identifier of the C `unsigned long` type.
    U64,

    /// The numeric identifier for the C `const char*` type.
    String,

    /// The numeric identifier for a C `const char*` type that should be read
    /// into a byte array.
    ByteArray,

    /// The numeric identifier of the C `size_t` type.
    SizeT,
}

impl Type {
    fn from_i64(value: i64) -> Result<Self, String> {
        match value {
            0 => Ok(Type::Void),
            1 => Ok(Type::Pointer),
            2 => Ok(Type::F64),
            3 => Ok(Type::F32),
            4 => Ok(Type::I8),
            5 => Ok(Type::I16),
            6 => Ok(Type::I32),
            7 => Ok(Type::I64),
            8 => Ok(Type::U8),
            9 => Ok(Type::U16),
            10 => Ok(Type::U32),
            11 => Ok(Type::U64),
            12 => Ok(Type::String),
            13 => Ok(Type::ByteArray),
            14 => Ok(Type::SizeT),
            _ => Err(format!("The type identifier '{}' is invalid", value)),
        }
    }

    unsafe fn as_ffi_type(&self) -> *mut ffi_type {
        match self {
            Type::Void => &mut types::void as *mut _,
            Type::Pointer => &mut types::pointer as *mut _,
            Type::F64 => &mut types::double as *mut _,
            Type::F32 => &mut types::float as *mut _,
            Type::I8 => &mut types::sint8 as *mut _,
            Type::I16 => &mut types::sint16 as *mut _,
            Type::I32 => &mut types::sint32 as *mut _,
            Type::I64 => &mut types::sint64 as *mut _,
            Type::U8 => &mut types::uint8 as *mut _,
            Type::U16 => &mut types::uint16 as *mut _,
            Type::U32 => &mut types::uint32 as *mut _,
            Type::U64 => &mut types::uint64 as *mut _,
            Type::String => &mut types::pointer as *mut _,
            Type::ByteArray => &mut types::pointer as *mut _,
            Type::SizeT => match mem::size_of::<usize>() {
                64 => &mut types::uint64 as *mut _,
                32 => &mut types::uint32 as *mut _,
                8 => &mut types::uint8 as *mut _,
                _ => &mut types::uint16 as *mut _,
            },
        }
    }
}

/// A C library, such as libc.
///
/// This is currently a thin wrapper around libloading's Library structure,
/// allowing us to decouple the rest of the VM code from libloading.
pub(crate) struct Library {
    inner: libloading::Library,
}

/// A pointer to an FFI type.
pub(crate) type TypePointer = *mut ffi_type;

/// A raw C pointer.
pub(crate) type RawPointer = *mut c_void;

/// A wrapper around a C pointer.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub(crate) struct Pointer {
    inner: RawPointer,
}

unsafe impl Send for Pointer {}

/// A function with a fixed number of arguments.
pub(crate) struct Function {
    /// The pointer to the function to call.
    pointer: Pointer,

    /// The CIF (Call Interface) to use for this function.
    cif: ffi_cif,

    /// The argument types of the function.
    argument_types: Vec<Type>,

    /// The raw FFI types of the arguments.
    ///
    /// A CIF maintains a reference to this array, so we have to keep it around.
    argument_ffi_types: Vec<TypePointer>,

    /// The return type of the function.
    return_type: Type,
}

/// Returns the size of a type ID.
///
/// The size of the type is returned as a tagged integer.
pub(crate) fn type_size(id: i64) -> Result<InkoPointer, String> {
    let size = unsafe {
        match Type::from_i64(id)? {
            Type::Void => types::void.size,
            Type::Pointer | Type::String | Type::ByteArray => {
                types::pointer.size
            }
            Type::F64 => types::double.size,
            Type::F32 => types::float.size,
            Type::I8 => types::sint8.size,
            Type::I16 => types::sint16.size,
            Type::I32 => types::sint32.size,
            Type::I64 => types::sint64.size,
            Type::U8 => types::uint8.size,
            Type::U16 => types::uint16.size,
            Type::U32 => types::uint32.size,
            Type::U64 => types::uint64.size,
            Type::SizeT => mem::size_of::<usize>(),
        }
    };

    Ok(InkoPointer::int(size as i64))
}

/// Returns the alignment of a type ID.
///
/// The alignment of the type is returned as a tagged integer.
pub(crate) fn type_alignment(id: i64) -> Result<InkoPointer, String> {
    let size = unsafe {
        match Type::from_i64(id)? {
            Type::Void => types::void.alignment,
            Type::Pointer | Type::String | Type::ByteArray => {
                types::pointer.alignment
            }
            Type::F64 => types::double.alignment,
            Type::F32 => types::float.alignment,
            Type::I8 => types::sint8.alignment,
            Type::I16 => types::sint16.alignment,
            Type::I32 => types::sint32.alignment,
            Type::I64 => types::sint64.alignment,
            Type::U8 => types::uint8.alignment,
            Type::U16 => types::uint16.alignment,
            Type::U32 => types::uint32.alignment,
            Type::U64 => types::uint64.alignment,
            Type::SizeT => mem::align_of::<usize>() as u16,
        }
    };

    Ok(InkoPointer::int(i64::from(size)))
}

/// A value of some sort to be passed to a C function.
pub(crate) enum Argument {
    F32(f32),
    F64(f64),
    I16(i16),
    I32(i32),
    I64(i64),
    I8(i8),
    Pointer(RawPointer),
    U16(u16),
    U32(u32),
    U64(u64),
    U8(u8),
    Void,
}

impl Argument {
    // Creates a new Argument wrapping the value of `ptr` according to the needs
    // of the FFI type specified in `ffi_type`.
    unsafe fn wrap(
        ffi_type: Type,
        ptr: InkoPointer,
    ) -> Result<Argument, String> {
        let argument = match ffi_type {
            Type::Pointer => Argument::Pointer(ptr.as_ptr() as RawPointer),
            Type::String => Argument::Pointer(
                ptr.get::<InkoString>().value().as_c_char_pointer()
                    as RawPointer,
            ),
            Type::ByteArray => Argument::Pointer(
                ptr.get::<ByteArray>().value().as_ptr() as RawPointer,
            ),
            Type::Void => Argument::Void,
            Type::F32 => Argument::F32(Float::read(ptr) as f32),
            Type::F64 => Argument::F64(Float::read(ptr)),
            Type::I8 => Argument::I8(Int::read(ptr) as i8),
            Type::I16 => Argument::I16(Int::read(ptr) as i16),
            Type::I32 => Argument::I32(Int::read(ptr) as i32),
            Type::I64 => Argument::I64(Int::read(ptr) as i64),
            Type::U8 => Argument::U8(Int::read(ptr) as u8),
            Type::U16 => Argument::U16(Int::read(ptr) as u16),
            Type::U32 => Argument::U32(Int::read(ptr) as u32),
            Type::U64 => Argument::U64(Int::read(ptr) as u64),
            Type::SizeT => match mem::size_of::<usize>() {
                64 => Argument::U64(Int::read(ptr) as u64),
                32 => Argument::U32(Int::read(ptr) as u32),
                8 => Argument::U8(Int::read(ptr) as u8),
                _ => Argument::U16(Int::read(ptr) as u16),
            },
        };

        Ok(argument)
    }

    /// Returns a C pointer to the wrapped value.
    fn as_c_pointer(&mut self) -> RawPointer {
        match self {
            Argument::Pointer(ref mut val) => {
                // When passing a pointer we shouldn't pass the pointer
                // directly, instead we want a pointer to the pointer to pass to
                // the underlying C function.
                val as *mut RawPointer as RawPointer
            }
            Argument::Void => ptr::null_mut() as RawPointer,
            Argument::F32(ref mut val) => val as *mut _ as RawPointer,
            Argument::F64(ref mut val) => val as *mut _ as RawPointer,
            Argument::I8(ref mut val) => val as *mut _ as RawPointer,
            Argument::I16(ref mut val) => val as *mut _ as RawPointer,
            Argument::I32(ref mut val) => val as *mut _ as RawPointer,
            Argument::I64(ref mut val) => val as *mut _ as RawPointer,
            Argument::U8(ref mut val) => val as *mut _ as RawPointer,
            Argument::U16(ref mut val) => val as *mut _ as RawPointer,
            Argument::U32(ref mut val) => val as *mut _ as RawPointer,
            Argument::U64(ref mut val) => val as *mut _ as RawPointer,
        }
    }
}

impl Library {
    /// Opens a library using one or more possible names, stored as pointers to
    /// heap allocated objects.
    pub(crate) fn from_pointers(search_for: &[InkoPointer]) -> Option<Library> {
        let names = search_for
            .iter()
            .map(|n| unsafe { InkoString::read(n) })
            .collect::<Vec<_>>();

        Self::open(&names)
    }

    /// Opens a library using one or more possible names.
    pub(crate) fn open<P: AsRef<OsStr> + Debug + Display>(
        search_for: &[P],
    ) -> Option<Library> {
        for name in search_for {
            if let Ok(library) = unsafe { libloading::Library::new(name) }
                .map(|inner| Library { inner })
            {
                return Some(library);
            }
        }

        None
    }

    /// Obtains a pointer to a symbol.
    ///
    /// This method is unsafe because the pointer could be of any type, thus it
    /// is up to the caller to make sure the result is used appropriately.
    pub(crate) unsafe fn get(&self, name: &str) -> Option<Pointer> {
        self.inner
            .get(name.as_bytes())
            .map(|sym: libloading::Symbol<RawPointer>| Pointer::new(*sym))
            .ok()
    }
}

impl Pointer {
    pub(crate) fn new(inner: RawPointer) -> Self {
        Pointer { inner }
    }

    /// Reads the value of this pointer into a particular type, based on the
    /// integer specified in `kind`.
    pub(crate) unsafe fn read_as(
        self,
        state: &State,
        kind: InkoPointer,
    ) -> Result<InkoPointer, String> {
        let int = Int::read(kind);
        let pointer = match Type::from_i64(int)? {
            Type::Pointer => {
                let ptr: RawPointer = self.read();

                InkoPointer::new(ptr as *mut u8)
            }
            Type::Void => InkoPointer::new(ptr::null_mut()),
            Type::String => {
                let string = self.read_cstr().to_string_lossy().into_owned();

                InkoString::alloc(state.permanent_space.string_class(), string)
            }
            Type::ByteArray => {
                let bytes = self.read_cstr().to_bytes().to_vec();

                ByteArray::alloc(
                    state.permanent_space.byte_array_class(),
                    bytes,
                )
            }
            Type::F64 => self.read_float::<c_double>(state),
            Type::F32 => self.read_float::<c_float>(state),
            Type::I8 | Type::U8 => self.read_signed_integer::<c_char>(state),
            Type::I16 | Type::U16 => self.read_signed_integer::<c_short>(state),
            Type::I32 | Type::U32 => self.read_signed_integer::<c_int>(state),
            Type::I64 | Type::U64 => self.read_signed_integer::<c_long>(state),
            Type::SizeT => match mem::size_of::<usize>() {
                64 => self.read_signed_integer::<c_long>(state),
                32 => self.read_signed_integer::<c_int>(state),
                8 => self.read_signed_integer::<c_char>(state),
                _ => self.read_signed_integer::<c_short>(state),
            },
        };

        Ok(pointer)
    }

    /// Writes a value to the underlying pointer.
    pub(crate) unsafe fn write_as(
        self,
        kind: InkoPointer,
        value: InkoPointer,
    ) -> Result<(), String> {
        let int = Int::read(kind);

        match Type::from_i64(int)? {
            Type::String => {
                let string = value.get::<InkoString>().value();

                ptr::copy(
                    string.as_c_char_pointer(),
                    self.inner as *mut c_char,
                    string.len_with_null_byte(),
                );
            }
            Type::ByteArray => {
                let byte_array = value.get::<ByteArray>().value();

                ptr::copy(
                    byte_array.as_ptr(),
                    self.inner as *mut _,
                    byte_array.len(),
                );
            }
            Type::Pointer => self.write(value.as_ptr() as RawPointer),
            Type::Void => self.write(ptr::null_mut() as RawPointer),
            Type::F64 => self.write(Float::read(value)),
            Type::F32 => self.write(Float::read(value) as f32),
            Type::I8 => self.write(Int::read(value) as i8),
            Type::I16 => self.write(Int::read(value) as i16),
            Type::I32 => self.write(Int::read(value) as i32),
            Type::I64 => self.write(Int::read(value) as i64),
            Type::U8 => self.write(Int::read(value) as u8),
            Type::U16 => self.write(Int::read(value) as u16),
            Type::U32 => self.write(Int::read(value) as u32),
            Type::U64 => self.write(Int::read(value) as u64),
            Type::SizeT => self.write(Int::read(value) as usize),
        };

        Ok(())
    }

    /// Returns a new Pointer, optionally starting at the given offset.
    ///
    /// The `offset` argument is the offset in _bytes_, not the number of
    /// elements (unlike Rust's `pointer::offset`).
    pub(crate) fn with_offset(self, offset_bytes: usize) -> Self {
        let inner = (self.inner as usize + offset_bytes) as RawPointer;

        Pointer::new(inner)
    }

    /// Returns the underlying pointer.
    pub(crate) fn as_ptr(self) -> *mut u8 {
        self.inner as _
    }

    unsafe fn read<R>(self) -> R {
        ptr::read(self.inner as *mut R)
    }

    unsafe fn write<T>(self, value: T) {
        ptr::write(self.inner as *mut T, value);
    }

    unsafe fn read_signed_integer<T: Into<i64>>(
        self,
        state: &State,
    ) -> InkoPointer {
        Int::alloc(state.permanent_space.int_class(), self.read::<T>().into())
    }

    unsafe fn read_float<T: Into<f64>>(self, state: &State) -> InkoPointer {
        Float::alloc(
            state.permanent_space.float_class(),
            self.read::<T>().into(),
        )
    }

    unsafe fn read_cstr<'a>(self) -> &'a CStr {
        CStr::from_ptr(self.inner as *mut c_char)
    }
}

impl Function {
    pub(crate) unsafe fn attach(
        library: &Library,
        name: &str,
        arguments: &[InkoPointer],
        return_type: InkoPointer,
    ) -> Result<Option<Function>, String> {
        let func_ptr = if let Some(sym) = library.get(name) {
            sym
        } else {
            return Ok(None);
        };

        let rtype = Type::from_i64(Int::read(return_type))?;
        let mut args = Vec::with_capacity(arguments.len());

        for ptr in arguments {
            args.push(Type::from_i64(Int::read(*ptr))?);
        }

        Self::create(func_ptr, args, rtype).map(Some)
    }

    unsafe fn create(
        pointer: Pointer,
        argument_types: Vec<Type>,
        return_type: Type,
    ) -> Result<Function, String> {
        let argument_ffi_types: Vec<TypePointer> =
            argument_types.iter().map(|t| t.as_ffi_type()).collect();
        let mut func = Function {
            pointer,
            cif: Default::default(),
            argument_types,
            argument_ffi_types,
            return_type,
        };

        let result = prep_cif(
            &mut func.cif,
            ABI,
            func.argument_types.len(),
            return_type.as_ffi_type(),
            func.argument_ffi_types.as_mut_ptr(),
        );

        match result {
            Ok(_) => Ok(func),
            Err(FFIError::Typedef) => {
                Err("The type representation is invalid or unsupported"
                    .to_string())
            }
            Err(FFIError::Abi) => {
                Err("The ABI is invalid or unsupported".to_string())
            }
        }
    }

    pub(crate) unsafe fn call(
        &self,
        state: &State,
        arg_ptrs: &[InkoPointer],
    ) -> Result<InkoPointer, String> {
        if arg_ptrs.len() != self.argument_types.len() {
            return Err(format!(
                "Invalid number of arguments, expected {} but got {}",
                self.argument_types.len(),
                arg_ptrs.len()
            ));
        }

        let mut arguments = Vec::with_capacity(arg_ptrs.len());

        for (index, arg) in arg_ptrs.iter().enumerate() {
            arguments.push(Argument::wrap(self.argument_types[index], *arg)?);
        }

        // libffi expects an array of _pointers_ to the arguments to pass,
        // instead of an array containing the arguments directly. The pointers
        // and the values they point to must outlive the FFI call, otherwise we
        // may end up passing pointers to invalid memory.
        let mut argument_pointers: Vec<RawPointer> =
            arguments.iter_mut().map(Argument::as_c_pointer).collect();

        // libffi requires a mutable pointer to the CIF, but "self" is immutable
        // since we never actually modify the current function. To work around
        // this we manually cast to a mutable pointer.
        let cif_ptr = &self.cif as *const _ as *mut _;
        let fun_ptr = CodePtr::from_ptr(self.pointer.inner);
        let args_ptr = argument_pointers.as_mut_ptr();
        let result = match self.return_type {
            Type::Void => {
                ffi_call::<c_void>(cif_ptr, fun_ptr, args_ptr);
                InkoPointer::nil_singleton()
            }
            Type::Pointer => {
                InkoPointer::new(ffi_call(cif_ptr, fun_ptr, args_ptr))
            }
            Type::F64 | Type::F32 => {
                let result: c_double = ffi_call(cif_ptr, fun_ptr, args_ptr);

                Float::alloc(state.permanent_space.float_class(), result as f64)
            }
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::SizeT => {
                let result: c_long = ffi_call(cif_ptr, fun_ptr, args_ptr);

                Int::alloc(state.permanent_space.int_class(), result as i64)
            }
            Type::String => {
                let result =
                    CStr::from_ptr(ffi_call(cif_ptr, fun_ptr, args_ptr))
                        .to_string_lossy()
                        .into_owned();

                InkoString::alloc(state.permanent_space.string_class(), result)
            }
            Type::ByteArray => {
                let result =
                    CStr::from_ptr(ffi_call(cif_ptr, fun_ptr, args_ptr))
                        .to_bytes();

                ByteArray::alloc(
                    state.permanent_space.byte_array_class(),
                    result.into(),
                )
            }
        };

        Ok(result)
    }
}

#[cfg(all(
    test,
    any(target_os = "macos", target_os = "linux", target_os = "windows")
))]
mod tests {
    use super::*;
    use crate::mem::Pointer as InkoPointer;
    use crate::mem::String as InkoString;
    use crate::test::setup;

    extern "C" {
        fn calloc(amount: usize, size: usize) -> RawPointer;
        fn free(pointer: RawPointer);
    }

    #[cfg(target_os = "macos")]
    const LIBM: &'static str = "libm.dylib";

    #[cfg(target_os = "linux")]
    const LIBM: &str = "libm.so.6";

    #[cfg(target_os = "windows")]
    const LIBM: &'static str = "msvcrt.dll";

    #[test]
    fn test_library_new() {
        assert!(Library::open(&[LIBM]).is_some());
    }

    #[test]
    fn test_library_get() {
        let lib = Library::open(&[LIBM]).unwrap();
        let sym = unsafe { lib.get("floor") };

        assert!(sym.is_some());
    }

    #[test]
    fn test_function_new() {
        let lib = Library::open(&[LIBM]).unwrap();

        unsafe {
            let sym = lib.get("floor").unwrap();

            let fun = Function::create(sym, vec![Type::F64], Type::F64);

            assert!(fun.is_ok());
        }
    }

    #[test]
    fn test_function_from_pointers() {
        let state = setup();
        let name = InkoString::alloc(
            state.permanent_space.string_class(),
            LIBM.to_string(),
        );

        let names = vec![name];
        let lib = Library::from_pointers(&names);

        assert!(lib.is_some());

        unsafe {
            InkoString::drop_and_deallocate(name);
        }
    }

    #[test]
    fn test_function_call() {
        let lib = Library::open(&[LIBM]).unwrap();
        let state = setup();
        let arg = Float::alloc(state.permanent_space.float_class(), 3.15);

        unsafe {
            let sym = lib.get("floor").unwrap();
            let fun =
                Function::create(sym, vec![Type::F64], Type::F64).unwrap();
            let res = fun.call(&state, &[arg]).unwrap();

            assert_eq!(Float::read(res), 3.0);
            arg.free();
            res.free();
        }
    }

    #[test]
    fn test_pointer_read_and_write() {
        let state = setup();

        unsafe {
            let ptr = Pointer::new(calloc(1, 3));
            let kind = InkoPointer::int(12);
            let val = InkoString::alloc(
                state.permanent_space.string_class(),
                "ab".to_string(),
            );

            ptr.write_as(kind, val).unwrap();

            let result = ptr.read_as(&state, kind);

            free(ptr.inner);

            assert!(result.is_ok());

            let new_string = result.unwrap();

            assert_eq!(InkoString::read(&new_string), "ab");

            InkoString::drop_and_deallocate(val);
            InkoString::drop_and_deallocate(new_string);
        }
    }
}

#[cfg(test)]
mod tests_for_all_platforms {
    use super::*;

    #[test]
    fn test_library_new_invalid() {
        let lib = Library::open(&["inko-test-1", "inko-test-2"]);

        assert!(lib.is_none());
    }
}
