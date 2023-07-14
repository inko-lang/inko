use std::alloc::{alloc, alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::mem::{align_of, forget, size_of, swap};
use std::ops::Deref;
use std::ptr::drop_in_place;
use std::slice;
use std::str;
use std::string::String as RustString;

/// The alignment to use for Inko objects.
const ALIGNMENT: usize = align_of::<usize>();

/// The number of bits to shift for tagged integers.
const INT_SHIFT: usize = 1;

/// The minimum integer value that can be stored as a tagged signed integer.
pub(crate) const MIN_INT: i64 = i64::MIN >> INT_SHIFT;

/// The maximum integer value that can be stored as a tagged signed integer.
pub(crate) const MAX_INT: i64 = i64::MAX >> INT_SHIFT;

/// The mask to use for tagged integers.
const INT_MASK: usize = 0b01;

/// A type indicating what sort of value we're dealing with in a certain place
/// (e.g. a ref or a permanent value).
///
/// The values of the variants are specified explicitly to make it more explicit
/// we depend on these exact values (e.g. in the compiler).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Kind {
    /// The value is a regular heap allocated, owned value.
    Owned = 0,

    /// The value is a reference to a heap allocated value.
    Ref = 1,

    /// The value is an owned value that uses atomic reference counting.
    Atomic = 2,

    /// The value musn't be dropped until the program stops.
    Permanent = 3,

    /// The value is a boxed Int.
    Int = 4,

    /// The value is a boxed Float.
    Float = 5,
}

pub(crate) fn allocate(layout: Layout) -> *mut u8 {
    unsafe {
        let ptr = alloc(layout);

        if ptr.is_null() {
            handle_alloc_error(layout);
        } else {
            ptr
        }
    }
}

fn with_mask<T>(ptr: *const T, mask: usize) -> *const T {
    (ptr as usize | mask) as _
}

fn mask_is_set<T>(ptr: *const T, mask: usize) -> bool {
    (ptr as usize & mask) == mask
}

pub(crate) unsafe fn header_of<'a, T>(ptr: *const T) -> &'a mut Header {
    &mut *(ptr as *mut Header)
}

pub(crate) fn is_tagged_int<T>(ptr: *const T) -> bool {
    mask_is_set(ptr, INT_MASK)
}

fn fits_in_tagged_int(value: i64) -> bool {
    (MIN_INT..=MAX_INT).contains(&value)
}

pub(crate) fn tagged_int(value: i64) -> *const Int {
    with_mask((value << INT_SHIFT) as *const Int, INT_MASK)
}

pub(crate) unsafe fn free<T>(ptr: *mut T) {
    let layout = header_of(ptr).class.instance_layout();

    dealloc(ptr as *mut u8, layout);
}

/// The header used by heap allocated objects.
///
/// The layout is fixed to ensure we can safely assume certain fields are at
/// certain offsets in an object, even when not knowing what type of object
/// we're dealing with.
#[repr(C)]
pub struct Header {
    /// The class of the object.
    pub class: ClassPointer,

    /// A flag indicating what kind of pointer/object we're dealing with.
    pub kind: Kind,

    /// The number of references to the object of this header.
    ///
    /// If this count overflows the program terminates. In practise this should
    /// never happen, as one needs _a lot_ of references to achieve this.
    ///
    /// We're using a u32 here instead of a u16, as the likelihood of
    /// overflowing a u32 is very tiny, but overflowing a u16 is something that
    /// _could_ happen (i.e. a process reference shared with many other
    /// processes).
    pub references: u32,
}

impl Header {
    pub(crate) fn init(&mut self, class: ClassPointer) {
        self.class = class;
        self.kind = Kind::Owned;
        self.references = 0;
    }

    pub(crate) fn init_atomic(&mut self, class: ClassPointer) {
        self.class = class;
        self.kind = Kind::Atomic;

        // Atomic values start with a reference count of 1, so
        // `decrement_atomic()` returns the correct result for a value for which
        // no extra references have been created (instead of overflowing).
        self.references = 1;
    }

    pub(crate) fn set_permanent(&mut self) {
        self.kind = Kind::Permanent;
    }

    pub(crate) fn is_permanent(&self) -> bool {
        matches!(self.kind, Kind::Permanent)
    }

    pub(crate) fn references(&self) -> u32 {
        self.references
    }
}

/// A function bound to an object.
///
/// Methods don't have headers as there's no need for any, as methods aren't
/// values one can pass around in Inko.
#[repr(C)]
pub struct Method {
    /// The hash of this method, used when performing dynamic dispatch.
    pub hash: u64,

    /// A pointer to the native function that backs this method.
    pub code: extern "system" fn(),
}

/// An Inko class.
#[repr(C)]
pub struct Class {
    /// The name of the class.
    pub(crate) name: RustString,

    /// The size (in bytes) of instances of this class.
    pub(crate) instance_size: u32,

    /// The number of method slots this class has.
    ///
    /// The actual number of methods may be less than this value.
    pub(crate) method_slots: u16,

    /// The methods of this class, as pointers to native functions.
    ///
    /// Methods are accessed frequently, and we want to do so with as little
    /// indirection and as cache-friendly as possible. For this reason we use a
    /// flexible array member, instead of a Vec.
    ///
    /// The length of this array _must_ be a power of two.
    pub methods: [Method; 0],
}

impl Class {
    pub(crate) unsafe fn drop(ptr: ClassPointer) {
        let layout = Self::layout(ptr.method_slots);
        let raw_ptr = ptr.0;

        drop_in_place(raw_ptr);
        dealloc(raw_ptr as *mut u8, layout);
    }

    pub(crate) fn alloc(
        name: RustString,
        methods: u16,
        size: u32,
    ) -> ClassPointer {
        let ptr = unsafe {
            let layout = Self::layout(methods);

            // For classes we zero memory out, so unused method slots are set to
            // zeroed memory, instead of random garbage.
            let ptr = alloc_zeroed(layout) as *mut Class;

            if ptr.is_null() {
                handle_alloc_error(layout);
            }

            ptr
        };
        let obj = unsafe { &mut *ptr };

        init!(obj.name => name);
        init!(obj.instance_size => size);
        init!(obj.method_slots => methods);

        ClassPointer(ptr)
    }

    /// Returns a new class for a regular object.
    pub(crate) fn object(
        name: RustString,
        size: u32,
        methods: u16,
    ) -> ClassPointer {
        Self::alloc(name, methods, size)
    }

    /// Returns a new class for a process.
    pub(crate) fn process(
        name: RustString,
        size: u32,
        methods: u16,
    ) -> ClassPointer {
        Self::alloc(name, methods, size)
    }

    /// Returns the `Layout` for a class itself.
    unsafe fn layout(methods: u16) -> Layout {
        let size =
            size_of::<Class>() + (methods as usize * size_of::<Method>());

        Layout::from_size_align_unchecked(size, align_of::<Class>())
    }

    pub(crate) unsafe fn instance_layout(&self) -> Layout {
        Layout::from_size_align_unchecked(
            self.instance_size as usize,
            ALIGNMENT,
        )
    }
}

/// A pointer to a class.
#[repr(transparent)]
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub struct ClassPointer(*mut Class);

impl Deref for ClassPointer {
    type Target = Class;

    fn deref(&self) -> &Class {
        unsafe { &*(self.0 as *const Class) }
    }
}

/// A resizable array of bytes.
#[repr(C)]
pub struct ByteArray {
    pub(crate) header: Header,
    pub(crate) value: Vec<u8>,
}

impl ByteArray {
    pub(crate) unsafe fn drop(ptr: *mut Self) {
        drop_in_place(ptr);
    }

    pub(crate) fn alloc(class: ClassPointer, value: Vec<u8>) -> *mut Self {
        let ptr = allocate(Layout::new::<Self>()) as *mut Self;
        let obj = unsafe { &mut *ptr };

        obj.header.init(class);
        init!(obj.value => value);
        ptr
    }

    pub(crate) fn take_bytes(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();

        swap(&mut bytes, &mut self.value);
        bytes
    }
}

/// A signed 64-bits integer.
#[repr(C)]
pub struct Int {
    pub(crate) header: Header,
    pub(crate) value: i64,
}

impl Int {
    pub(crate) fn new(class: ClassPointer, value: i64) -> *const Self {
        if fits_in_tagged_int(value) {
            tagged_int(value)
        } else {
            Self::boxed(class, value)
        }
    }

    pub(crate) fn new_permanent(
        class: ClassPointer,
        value: i64,
    ) -> *const Self {
        if fits_in_tagged_int(value) {
            tagged_int(value)
        } else {
            Self::boxed_permanent(class, value)
        }
    }

    pub(crate) fn boxed(class: ClassPointer, value: i64) -> *const Self {
        let ptr = allocate(Layout::new::<Self>()) as *mut Self;
        let obj = unsafe { &mut *ptr };

        obj.header.init(class);
        obj.header.kind = Kind::Int;
        init!(obj.value => value);
        ptr as _
    }

    pub(crate) fn boxed_permanent(
        class: ClassPointer,
        value: i64,
    ) -> *const Self {
        let ptr = allocate(Layout::new::<Self>()) as *mut Self;
        let obj = unsafe { &mut *ptr };

        obj.header.init(class);
        obj.header.set_permanent();
        init!(obj.value => value);
        ptr as _
    }
}

#[repr(C)]
pub struct Bool {
    pub(crate) header: Header,
}

impl Bool {
    pub(crate) fn drop_and_deallocate(ptr: *const Self) {
        unsafe {
            drop_in_place(ptr as *mut Self);
            dealloc(ptr as *mut u8, Layout::new::<Self>());
        }
    }

    pub(crate) fn alloc(class: ClassPointer) -> *const Self {
        let ptr = allocate(Layout::new::<Self>()) as *mut Self;
        let obj = unsafe { &mut *ptr };

        obj.header.init(class);
        obj.header.set_permanent();
        ptr as _
    }
}

#[repr(C)]
pub struct Nil {
    pub(crate) header: Header,
}

impl Nil {
    pub(crate) fn drop_and_deallocate(ptr: *const Self) {
        unsafe {
            drop_in_place(ptr as *mut Self);
            dealloc(ptr as *mut u8, Layout::new::<Self>());
        }
    }

    pub(crate) fn alloc(class: ClassPointer) -> *const Self {
        let ptr = allocate(Layout::new::<Self>()) as *mut Self;
        let obj = unsafe { &mut *ptr };

        obj.header.init(class);
        obj.header.set_permanent();
        ptr as _
    }
}

/// A heap allocated float.
#[repr(C)]
pub struct Float {
    pub(crate) header: Header,
    pub(crate) value: f64,
}

impl Float {
    pub(crate) fn alloc(class: ClassPointer, value: f64) -> *const Float {
        let ptr = allocate(Layout::new::<Self>()) as *mut Self;
        let obj = unsafe { &mut *ptr };

        obj.header.init(class);
        obj.header.kind = Kind::Float;
        init!(obj.value => value);
        ptr as _
    }

    pub(crate) fn alloc_permanent(
        class: ClassPointer,
        value: f64,
    ) -> *const Float {
        let ptr = Self::alloc(class, value);

        unsafe { header_of(ptr) }.set_permanent();
        ptr
    }
}

/// A heap allocated string.
///
/// Strings use atomic reference counting as they are treated as value types,
/// and this removes the need for cloning the string's contents (at the cost of
/// atomic operations).
#[repr(C)]
pub struct String {
    pub header: Header,
    pub size: u64,
    pub bytes: *mut u8,
}

impl String {
    pub(crate) unsafe fn drop(ptr: *const Self) {
        drop_in_place(ptr as *mut Self);
    }

    pub(crate) unsafe fn drop_and_deallocate(ptr: *const Self) {
        Self::drop(ptr);
        free(ptr as *mut u8);
    }

    pub(crate) unsafe fn read<'a>(ptr: *const String) -> &'a str {
        (*ptr).as_slice()
    }

    pub(crate) fn alloc(
        class: ClassPointer,
        value: RustString,
    ) -> *const String {
        Self::new(class, value.into_bytes())
    }

    pub(crate) fn alloc_permanent(
        class: ClassPointer,
        value: RustString,
    ) -> *const String {
        let ptr = Self::new(class, value.into_bytes());

        unsafe { header_of(ptr) }.set_permanent();
        ptr
    }

    pub(crate) fn from_bytes(
        class: ClassPointer,
        bytes: Vec<u8>,
    ) -> *const String {
        let string = match RustString::from_utf8(bytes) {
            Ok(string) => string,
            Err(err) => {
                RustString::from_utf8_lossy(&err.into_bytes()).into_owned()
            }
        };

        String::new(class, string.into_bytes())
    }

    fn new(class: ClassPointer, mut bytes: Vec<u8>) -> *const String {
        let len = bytes.len();

        bytes.reserve_exact(1);
        bytes.push(0);

        // Vec and Box<[u8]> don't have a public/stable memory layout. To work
        // around that we have to break the Vec apart into a buffer and length,
        // and store the two separately.
        let mut boxed = bytes.into_boxed_slice();
        let buffer = boxed.as_mut_ptr();

        forget(boxed);

        let ptr = allocate(Layout::new::<Self>()) as *mut Self;
        let obj = unsafe { &mut *ptr };

        obj.header.init_atomic(class);
        init!(obj.size => len as u64);
        init!(obj.bytes => buffer);
        ptr as _
    }

    /// Returns a string slice pointing to the underlying bytes.
    ///
    /// The returned slice _does not_ include the NULL byte.
    pub(crate) fn as_slice(&self) -> &str {
        unsafe { str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Returns a slice to the underlying bytes, without the NULL byte.
    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.bytes, self.size as usize) }
    }
}

impl Drop for String {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(slice::from_raw_parts_mut(
                self.bytes,
                (self.size + 1) as usize,
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};
    use std::ptr::addr_of;

    extern "system" fn dummy() {}

    #[test]
    fn test_header_field_offsets() {
        let header = Header {
            class: ClassPointer(0x7 as _),
            kind: Kind::Owned,
            references: 42,
        };

        let base = addr_of!(header) as usize;

        assert_eq!(addr_of!(header.class) as usize - base, 0);
        assert_eq!(addr_of!(header.kind) as usize - base, 8);
        assert_eq!(addr_of!(header.references) as usize - base, 12);
    }

    #[test]
    fn test_class_field_offsets() {
        let class = Class::alloc("A".to_string(), 4, 8);
        let base = class.0 as usize;

        assert_eq!(addr_of!(class.name) as usize - base, 0);
        assert_eq!(addr_of!(class.instance_size) as usize - base, 24);
        assert_eq!(addr_of!(class.method_slots) as usize - base, 28);
        assert_eq!(addr_of!(class.methods) as usize - base, 32);

        unsafe {
            Class::drop(class);
        }
    }

    #[test]
    fn test_method_field_offsets() {
        let method = Method { hash: 42, code: dummy };
        let base = addr_of!(method) as usize;

        assert_eq!(addr_of!(method.hash) as usize - base, 0);
        assert_eq!(addr_of!(method.code) as usize - base, 8);
    }

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<Header>(), 16);
        assert_eq!(size_of::<Method>(), 16);
        assert_eq!(size_of::<Int>(), 24);
        assert_eq!(size_of::<Float>(), 24);
        assert_eq!(size_of::<String>(), 32);
        assert_eq!(size_of::<ByteArray>(), 40);
        assert_eq!(size_of::<Method>(), 16);
        assert_eq!(size_of::<Class>(), 32);
    }

    #[test]
    fn test_type_alignments() {
        assert_eq!(align_of::<Header>(), ALIGNMENT);
        assert_eq!(align_of::<Int>(), ALIGNMENT);
        assert_eq!(align_of::<Float>(), ALIGNMENT);
        assert_eq!(align_of::<String>(), ALIGNMENT);
        assert_eq!(align_of::<ByteArray>(), ALIGNMENT);
        assert_eq!(align_of::<Method>(), ALIGNMENT);
        assert_eq!(align_of::<Class>(), ALIGNMENT);
    }

    #[test]
    fn test_with_mask() {
        let ptr = with_mask(0x4 as *const u8, 0b10);

        assert_eq!(ptr as usize, 0x6);
    }

    #[test]
    fn test_class_new() {
        let class = Class::alloc("A".to_string(), 0, 24);

        assert_eq!(class.method_slots, 0);
        assert_eq!(class.instance_size, 24);

        unsafe { Class::drop(class) };
    }

    #[test]
    fn test_class_new_object() {
        let class = Class::object("A".to_string(), 24, 0);

        assert_eq!(class.method_slots, 0);
        assert_eq!(class.instance_size, 24);

        unsafe { Class::drop(class) };
    }

    #[test]
    fn test_class_new_process() {
        let class = Class::process("A".to_string(), 24, 0);

        assert_eq!(class.method_slots, 0);
        assert_eq!(class.instance_size, 24);

        unsafe { Class::drop(class) };
    }

    #[test]
    fn test_string_new() {
        let class = Class::object("A".to_string(), 24, 0);
        let string = String::new(class, vec![105, 110, 107, 111]);

        unsafe {
            assert_eq!((*string).as_bytes(), &[105, 110, 107, 111]);
            assert_eq!(String::read(string), "inko");
            Class::drop(class);
        }
    }

    #[test]
    fn test_string_from_bytes() {
        let class = Class::object("A".to_string(), 24, 0);
        let string = String::from_bytes(
            class,
            vec![
                72, 101, 108, 108, 111, 32, 240, 144, 128, 87, 111, 114, 108,
                100,
            ],
        );

        unsafe {
            assert_eq!(String::read(string), "Hello �World");
            Class::drop(class);
        }
    }
}
