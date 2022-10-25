use crate::immutable_string::ImmutableString;
use crate::indexes::*;
use crate::location_table::LocationTable;
use crate::permanent_space::PermanentSpace;
use crate::process::Process;
use bytecode::Instruction;
use std::alloc::{alloc, alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::mem::{align_of, size_of, swap, transmute};
use std::ops::Deref;
use std::ptr::drop_in_place;
use std::string::String as RustString;
use std::sync::atomic::{AtomicU16, Ordering};

/// The alignment to use for Inko objects.
const ALIGNMENT: usize = align_of::<usize>();

/// The number of bits to shift for tagged integers.
///
/// We shift by two bits, limiting tagged integers to 62 bits. This frees up the
/// lower two bits for non tagged values. We need two bits instead of one so we
/// can efficiently tell the various immediate values apart.
const INT_SHIFT_BITS: usize = 2;

/// The minimum integer value that can be stored as a tagged signed integer.
pub(crate) const MIN_INTEGER: i64 = i64::MIN >> INT_SHIFT_BITS;

/// The maximum integer value that can be stored as a tagged signed integer.
pub(crate) const MAX_INTEGER: i64 = i64::MAX >> INT_SHIFT_BITS;

/// The bit set for all immediate values.
const IMMEDIATE_BIT: usize = 0b1;

/// The mask to use for tagged integers.
const INT_MASK: usize = 0b00_0011;

/// The mask to use for detecting booleans.
const BOOL_MASK: usize = 0b00_0101;

/// The address of the singleton `False`.
const FALSE_ADDRESS: usize = 0b00_0101;

/// The address of the singleton `True`.
const TRUE_ADDRESS: usize = 0b00_1101;

/// The address of the `Nil` singleton.
const NIL_ADDRESS: usize = 0b00_0001;

/// The address of the `undefined` singleton.
const UNDEFINED_ADDRESS: usize = 0b00_1001;

/// The mask to apply for permanent objects.
const PERMANENT_MASK: usize = 0b00_0010;

/// The mask to apply for references.
const REF_MASK: usize = 0b00_0100;

/// The mask to use for detecting values that are not immediate or permanent
/// values.
const LOCAL_OWNED_MASK: usize = 0b00_0011;

/// The mask to use for untagging a pointer.
const UNTAG_MASK: usize = (!0b111) as usize;

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

/// A pointer to an object managed by the Inko runtime.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct Pointer(*mut u8);

unsafe impl Sync for Pointer {}
unsafe impl Send for Pointer {}

impl Pointer {
    /// Creates a new Pointer from the raw address.
    pub(crate) fn new(raw: *mut u8) -> Self {
        Self(raw)
    }

    /// Creates a pointer to a regular boxed value.
    ///
    /// This method is intended to be used when we want to pretend a Rust value
    /// is an Inko object. This allows exposing of Rust data to Inko, without
    /// having to wrap it in a heap object.
    pub(crate) fn boxed<T>(value: T) -> Self {
        Self::new(Box::into_raw(Box::new(value)) as *mut u8)
    }

    pub(crate) fn with_mask(raw: *mut u8, mask: usize) -> Self {
        Self::new((raw as usize | mask) as _)
    }

    pub(crate) fn int(value: i64) -> Self {
        Self::with_mask((value << INT_SHIFT_BITS) as _, INT_MASK)
    }

    pub(crate) fn true_singleton() -> Self {
        Self::new(TRUE_ADDRESS as _)
    }

    pub(crate) fn false_singleton() -> Self {
        Self::new(FALSE_ADDRESS as _)
    }

    pub(crate) fn nil_singleton() -> Self {
        Self::new(NIL_ADDRESS as _)
    }

    pub(crate) fn undefined_singleton() -> Self {
        Self::new(UNDEFINED_ADDRESS as _)
    }

    pub(crate) fn is_regular(self) -> bool {
        (self.as_ptr() as usize & IMMEDIATE_BIT) == 0
    }

    pub(crate) fn is_boolean(self) -> bool {
        self.mask_is_set(BOOL_MASK)
    }

    pub(crate) fn is_tagged_int(self) -> bool {
        self.mask_is_set(INT_MASK)
    }

    pub(crate) fn is_permanent(self) -> bool {
        self.mask_is_set(PERMANENT_MASK)
    }

    pub(crate) fn is_ref(self) -> bool {
        self.mask_is_set(REF_MASK)
    }

    /// Returns a boolean indicating if the pointer points to a non-permanent
    /// heap object.
    pub(crate) fn is_local_heap_object(self) -> bool {
        (self.as_ptr() as usize & LOCAL_OWNED_MASK) == 0
    }

    pub(crate) fn as_permanent(self) -> Pointer {
        Self::with_mask(self.as_ptr(), PERMANENT_MASK)
    }

    pub(crate) fn as_ref(self) -> Pointer {
        Self::with_mask(self.as_ptr(), REF_MASK)
    }

    pub(crate) fn as_ptr(self) -> *mut u8 {
        self.0
    }

    pub(crate) unsafe fn get<'a, T>(self) -> &'a T {
        &*(self.untagged_ptr() as *const T)
    }

    pub(crate) unsafe fn get_mut<'a, T>(self) -> &'a mut T {
        &mut *(self.untagged_ptr() as *mut T)
    }

    /// Drops and deallocates the object this pointer points to.
    ///
    /// This method is meant to be used only when a Pointer points to a value
    /// allocated using Rust's Box type.
    pub(crate) unsafe fn drop_boxed<T>(self) {
        drop(Box::from_raw(self.as_ptr() as *mut T));
    }

    pub(crate) fn untagged_ptr(self) -> *mut u8 {
        (self.as_ptr() as usize & UNTAG_MASK) as _
    }

    pub(crate) unsafe fn as_int(self) -> i64 {
        self.as_ptr() as i64 >> INT_SHIFT_BITS
    }

    pub(crate) unsafe fn free(self) {
        let header = self.get::<Header>();
        let layout = header.class.instance_layout();

        dealloc(self.untagged_ptr(), layout);
    }

    pub(crate) fn mask_is_set(self, mask: usize) -> bool {
        (self.as_ptr() as usize & mask) == mask
    }
}

/// The header used by heap allocated objects.
///
/// The layout is fixed to ensure we can safely assume certain fields are at
/// certain offsets in an object, even when not knowing what type of object
/// we're dealing with.
#[repr(C)]
pub(crate) struct Header {
    /// The class of the object.
    pub(crate) class: ClassPointer,

    /// A flag indicating the object uses atomic reference counting.
    atomic: bool,

    /// The number of references to the object of this header.
    ///
    /// A 16-bits integer should be enough for every program.
    ///
    /// Not all objects use reference counting, but we still reserve the space
    /// in a header. This makes it easier for generic code to handle both
    /// objects that do and don't use reference counting.
    references: u16,
}

impl Header {
    pub(crate) fn init(&mut self, class: ClassPointer) {
        self.class = class;
        self.atomic = false;
        self.references = 0;
    }

    pub(crate) fn init_atomic(&mut self, class: ClassPointer) {
        self.class = class;
        self.atomic = true;

        // Atomic values start with a reference count of 1, so
        // `decrement_atomic()` returns the correct result for a value for which
        // no extra references have been created (instead of overflowing).
        self.references = 1;
    }

    pub(crate) fn is_atomic(&self) -> bool {
        self.atomic
    }

    pub(crate) fn references(&self) -> u16 {
        self.references
    }

    pub(crate) fn increment(&mut self) {
        self.references += 1;
    }

    pub(crate) fn decrement(&mut self) {
        debug_assert_ne!(self.references, 0);

        self.references -= 1;
    }

    pub(crate) fn increment_atomic(&self) {
        self.references_as_atomic().fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn decrement_atomic(&self) -> bool {
        let old = self.references_as_atomic().fetch_sub(1, Ordering::AcqRel);

        // fetch_sub() overflows, making it harder to detect errors during
        // development.
        debug_assert_ne!(old, 0);

        old == 1
    }

    fn references_as_atomic(&self) -> &AtomicU16 {
        unsafe { transmute::<_, &AtomicU16>(&self.references) }
    }
}

/// A method bound to an object.
///
/// Methods aren't values and can't be passed around, nor can you call methods
/// on them. As such, methods don't have headers or classes.
#[repr(C)]
pub(crate) struct Method {
    /// The hash of this method, used when performing dynamic dispatch.
    ///
    /// We use a u32 as this is easier to encode into an instruction compared to
    /// a u64.
    pub(crate) hash: u32,
    pub(crate) registers: u16,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) locations: LocationTable,
    pub(crate) jump_tables: Vec<Vec<usize>>,
}

impl Method {
    pub(crate) fn drop_and_deallocate(ptr: MethodPointer) {
        unsafe {
            drop_in_place(ptr.as_ptr());
            dealloc(ptr.as_ptr() as *mut u8, Self::layout());
        }
    }

    pub(crate) fn alloc(
        hash: u32,
        registers: u16,
        instructions: Vec<Instruction>,
        locations: LocationTable,
        jump_tables: Vec<Vec<usize>>,
    ) -> MethodPointer {
        unsafe {
            let ptr = allocate(Self::layout()) as *mut Self;
            let obj = &mut *ptr;

            init!(obj.hash => hash);
            init!(obj.registers => registers);
            init!(obj.instructions => instructions);
            init!(obj.locations => locations);
            init!(obj.jump_tables => jump_tables);

            MethodPointer(ptr)
        }
    }

    unsafe fn layout() -> Layout {
        Layout::from_size_align_unchecked(
            size_of::<Method>(),
            align_of::<Method>(),
        )
    }
}

/// A pointer to an immutable method.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub(crate) struct MethodPointer(*mut Method);

impl MethodPointer {
    /// Returns the MethodPointer as a regular Pointer.
    pub(crate) fn as_ptr(self) -> *mut Method {
        self.0
    }
}

impl Deref for MethodPointer {
    type Target = Method;

    fn deref(&self) -> &Method {
        unsafe { &*(self.0 as *const Method) }
    }
}

/// An Inko class.
///
/// Classes come in a variety of sizes, and we don't drop them while the program
/// is running. To make managing memory easier, classes are always allocated
/// using the system allocator.
///
/// Due to the size of this type being variable, it's used/allocated using the
/// Class type, which acts like an owned pointer to this data.
#[repr(C)]
pub(crate) struct Class {
    /// The header of this class.
    ///
    /// The class pointer in this header will point to this class itself.
    header: Header,

    /// The name of the class.
    pub(crate) name: RustString,

    /// The size (in bytes) of instances of this class.
    pub(crate) instance_size: usize,

    /// The number of method slots this class has.
    ///
    /// The actual number of methods may be less than this value.
    pub(crate) method_slots: u16,

    /// All the methods of this class.
    ///
    /// Methods are accessed frequently, and we want to do so with as little
    /// indirection and as cache-friendly as possible. For this reason we use a
    /// flexible array member, instead of a Vec.
    ///
    /// The length of this table is always a power of two, which means some
    /// slots are NULL.
    methods: [MethodPointer; 0],
}

impl Class {
    pub(crate) fn drop(ptr: ClassPointer) {
        unsafe {
            let layout = Self::layout(ptr.method_slots);
            let raw_ptr = ptr.as_ptr();

            drop_in_place(raw_ptr);
            dealloc(raw_ptr as *mut u8, layout);
        }
    }

    pub(crate) fn alloc(
        name: RustString,
        methods: u16,
        size: usize,
    ) -> ClassPointer {
        let mut class_ptr = unsafe {
            let layout = Self::layout(methods);

            // For classes we zero memory out, so unused method slots are set to
            // zeroed memory, instead of random garbage.
            let ptr = alloc_zeroed(layout) as *mut Class;

            if ptr.is_null() {
                handle_alloc_error(layout);
            }

            ClassPointer::new(ptr)
        };
        let class_ptr_copy = class_ptr;
        let class = unsafe { class_ptr.get_mut() };

        class.header.init(class_ptr_copy);

        init!(class.name => name);
        init!(class.instance_size => size);
        init!(class.method_slots => methods);

        class_ptr
    }

    /// Returns a new class for a regular object.
    pub(crate) fn object(
        name: RustString,
        fields: usize,
        methods: u16,
    ) -> ClassPointer {
        let size = size_of::<Object>() + (fields * size_of::<Pointer>());

        Self::alloc(name, methods, size)
    }

    /// Returns a new class for a process.
    pub(crate) fn process(
        name: RustString,
        fields: usize,
        methods: u16,
    ) -> ClassPointer {
        let size = size_of::<Process>() + (fields * size_of::<Pointer>());

        Self::alloc(name, methods, size)
    }

    /// Returns a pointer to the class of the given pointer.
    pub(crate) fn of(space: &PermanentSpace, ptr: Pointer) -> ClassPointer {
        if ptr.is_regular() {
            unsafe { ptr.get::<Header>().class }
        } else if ptr.is_tagged_int() {
            space.int_class()
        } else if ptr.is_boolean() {
            space.boolean_class()
        } else {
            space.nil_class()
        }
    }

    /// Returns the `Layout` for a class itself.
    unsafe fn layout(methods: u16) -> Layout {
        let size =
            size_of::<Class>() + (methods as usize * size_of::<Pointer>());

        Layout::from_size_align_unchecked(size, align_of::<Class>())
    }

    pub(crate) unsafe fn instance_layout(&self) -> Layout {
        Layout::from_size_align_unchecked(self.instance_size, ALIGNMENT)
    }

    pub(crate) fn set_method(
        &mut self,
        index: MethodIndex,
        value: MethodPointer,
    ) {
        unsafe { self.methods.as_mut_ptr().add(index.into()).write(value) };
    }

    pub(crate) unsafe fn get_method(
        &self,
        index: MethodIndex,
    ) -> MethodPointer {
        *self.methods.as_ptr().add(index.into())
    }

    /// Look up a method using hashing.
    ///
    /// This method is useful for dynamic dispatch, as an exact offset isn't
    /// known in such cases. For this to work, each unique method name must have
    /// its own unique hash. This method won't work if two different methods
    /// have the same hash.
    ///
    /// In addition, we require that the number of methods in our class is a
    /// power of 2, as this allows the use of a bitwise AND instead of the
    /// modulo operator.
    ///
    /// Finally, similar to `get_method()` we expect there to be a method for
    /// the given hash. In practise this is always the case as the compiler
    /// enforces this, hence we don't check for this explicitly.
    ///
    /// For more information on this technique, refer to
    /// https://thume.ca/2019/07/29/shenanigans-with-hash-tables/.
    pub(crate) unsafe fn get_hashed_method(
        &self,
        input_hash: u32,
    ) -> MethodPointer {
        let len = (self.method_slots - 1) as u32;
        let mut index = input_hash;

        loop {
            index &= len;

            // The cast to a u16 is safe here, as the above &= ensures we limit
            // the hash value to the method count.
            let ptr = self.get_method(MethodIndex::new(index as u16));

            if ptr.hash == input_hash {
                return ptr;
            }

            index += 1;
        }
    }
}

impl Drop for Class {
    fn drop(&mut self) {
        for index in 0..self.method_slots {
            let method = unsafe { self.get_method(MethodIndex::new(index)) };

            if method.as_ptr().is_null() {
                // Because the table size is always a power of two, some slots
                // may be NULL.
                continue;
            }

            Method::drop_and_deallocate(method);
        }
    }
}

/// A pointer to a class.
#[repr(transparent)]
#[derive(Eq, PartialEq, Copy, Clone)]
pub(crate) struct ClassPointer(*mut Class);

impl ClassPointer {
    /// Returns a new ClassPointer from a raw Pointer.
    ///
    /// This method is unsafe as it doesn't perform any checks to ensure the raw
    /// pointer actually points to a class.
    pub(crate) unsafe fn new(pointer: *mut Class) -> Self {
        Self(pointer)
    }

    /// Sets a method in the given index.
    pub(crate) unsafe fn set_method(
        mut self,
        index: MethodIndex,
        value: MethodPointer,
    ) {
        self.get_mut().set_method(index, value);
    }

    pub(crate) fn as_ptr(self) -> *mut Class {
        self.0
    }

    /// Returns a mutable reference to the underlying class.
    ///
    /// This method is unsafe because no synchronisation is applied, nor do we
    /// guarantee there's only a single writer.
    unsafe fn get_mut(&mut self) -> &mut Class {
        &mut *(self.0 as *mut Class)
    }
}

impl Deref for ClassPointer {
    type Target = Class;

    fn deref(&self) -> &Class {
        unsafe { &*(self.0 as *const Class) }
    }
}

/// A resizable array.
#[repr(C)]
pub(crate) struct Array {
    header: Header,
    value: Vec<Pointer>,
}

impl Array {
    /// Drops the given Array.
    ///
    /// This method is unsafe as it doesn't check if the object is actually an
    /// Array.
    pub(crate) unsafe fn drop(ptr: Pointer) {
        drop_in_place(ptr.untagged_ptr() as *mut Self);
    }

    pub(crate) fn alloc(class: ClassPointer, value: Vec<Pointer>) -> Pointer {
        let ptr = Pointer::new(allocate(Layout::new::<Self>()));
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);
        init!(obj.value => value);
        ptr
    }

    pub(crate) fn value(&self) -> &Vec<Pointer> {
        &self.value
    }

    pub(crate) fn value_mut(&mut self) -> &mut Vec<Pointer> {
        &mut self.value
    }
}

/// A resizable array of bytes.
#[repr(C)]
pub(crate) struct ByteArray {
    header: Header,
    value: Vec<u8>,
}

impl ByteArray {
    /// Drops the given ByteArray.
    ///
    /// This method is unsafe as it doesn't check if the object is actually an
    /// ByteArray.
    pub(crate) unsafe fn drop(ptr: Pointer) {
        drop_in_place(ptr.untagged_ptr() as *mut Self);
    }

    pub(crate) fn alloc(class: ClassPointer, value: Vec<u8>) -> Pointer {
        let ptr = Pointer::new(allocate(Layout::new::<Self>()));
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);
        init!(obj.value => value);
        ptr
    }

    pub(crate) fn value(&self) -> &Vec<u8> {
        &self.value
    }

    pub(crate) fn value_mut(&mut self) -> &mut Vec<u8> {
        &mut self.value
    }

    pub(crate) fn take_bytes(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();

        swap(&mut bytes, &mut self.value);
        bytes
    }
}

/// A signed 64-bits integer.
#[repr(C)]
pub(crate) struct Int {
    header: Header,
    value: i64,
}

impl Int {
    pub(crate) fn alloc(class: ClassPointer, value: i64) -> Pointer {
        if (MIN_INTEGER..=MAX_INTEGER).contains(&value) {
            return Pointer::int(value);
        }

        let ptr = Pointer::new(allocate(Layout::new::<Self>()));
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);
        init!(obj.value => value);
        ptr
    }

    pub(crate) unsafe fn read(ptr: Pointer) -> i64 {
        if ptr.is_tagged_int() {
            ptr.as_int()
        } else {
            ptr.get::<Int>().value
        }
    }

    pub(crate) unsafe fn read_u64(ptr: Pointer) -> u64 {
        let val = Self::read(ptr);

        if val < 0 {
            0
        } else {
            val as u64
        }
    }
}

/// A heap allocated float.
#[repr(C)]
pub(crate) struct Float {
    header: Header,
    value: f64,
}

impl Float {
    pub(crate) fn alloc(class: ClassPointer, value: f64) -> Pointer {
        let ptr = Pointer::new(allocate(Layout::new::<Self>()));
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);
        init!(obj.value => value);
        ptr
    }

    /// Reads the float value from the pointer.
    ///
    /// If the pointer doesn't actually point to a float, the behaviour is
    /// undefined.
    pub(crate) unsafe fn read(ptr: Pointer) -> f64 {
        ptr.get::<Self>().value
    }
}

/// A heap allocated string.
///
/// Strings use atomic reference counting as they are treated as value types,
/// and this removes the need for cloning the string's contents (at the cost of
/// atomic operations).
#[repr(C)]
pub(crate) struct String {
    header: Header,
    value: ImmutableString,
}

impl String {
    pub(crate) unsafe fn drop(ptr: Pointer) {
        drop_in_place(ptr.untagged_ptr() as *mut Self);
    }

    pub(crate) unsafe fn drop_and_deallocate(ptr: Pointer) {
        Self::drop(ptr);
        ptr.free();
    }

    pub(crate) unsafe fn read(ptr: &Pointer) -> &str {
        ptr.get::<Self>().value().as_slice()
    }

    pub(crate) fn alloc(class: ClassPointer, value: RustString) -> Pointer {
        Self::from_immutable_string(class, ImmutableString::from(value))
    }

    pub(crate) fn from_immutable_string(
        class: ClassPointer,
        value: ImmutableString,
    ) -> Pointer {
        let ptr = Pointer::new(allocate(Layout::new::<Self>()));
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init_atomic(class);
        init!(obj.value => value);
        ptr
    }

    pub(crate) fn value(&self) -> &ImmutableString {
        &self.value
    }
}

/// A module containing classes, methods, and code to run.
#[repr(C)]
pub(crate) struct Module {
    header: Header,
}

unsafe impl Send for Module {}

impl Module {
    pub(crate) fn drop_and_deallocate(ptr: ModulePointer) {
        unsafe {
            drop_in_place(ptr.as_pointer().untagged_ptr() as *mut Self);
            ptr.as_pointer().free();
        }
    }

    pub(crate) fn alloc(class: ClassPointer) -> ModulePointer {
        let ptr = allocate(Layout::new::<Self>());

        unsafe { &mut *(ptr as *mut Self) }.header.init(class);
        ModulePointer(ptr)
    }

    pub(crate) fn name(&self) -> &RustString {
        &self.header.class.name
    }
}

/// A pointer to a module.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub(crate) struct ModulePointer(*mut u8);

impl ModulePointer {
    pub(crate) fn as_pointer(self) -> Pointer {
        Pointer::new(self.0).as_permanent()
    }
}

impl Deref for ModulePointer {
    type Target = Module;

    fn deref(&self) -> &Module {
        unsafe { &*(self.0 as *const Module) }
    }
}

/// A regular object that can store zero or more fields.
///
/// The size of this object varies based on the number of fields it has to
/// store.
#[repr(C)]
pub(crate) struct Object {
    header: Header,

    /// The fields of this object.
    ///
    /// The length of this flexible array is derived from the number of
    /// fields defined in this object's class.
    fields: [Pointer; 0],
}

impl Object {
    /// Bump allocates a user-defined object of a variable size.
    pub(crate) fn alloc(class: ClassPointer) -> Pointer {
        let ptr = Pointer::new(allocate(unsafe { class.instance_layout() }));
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);
        ptr
    }

    pub(crate) unsafe fn set_field(
        &mut self,
        index: FieldIndex,
        value: Pointer,
    ) {
        self.fields.as_mut_ptr().add(index.into()).write(value)
    }

    pub(crate) unsafe fn get_field(&self, index: FieldIndex) -> Pointer {
        *self.fields.as_ptr().add(index.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{empty_method, empty_module, OwnedClass};
    use std::mem::{align_of, size_of};

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<Header>(), 16);
        assert_eq!(size_of::<Object>(), 16); // variable, based on the fields

        assert_eq!(size_of::<Int>(), 24);
        assert_eq!(size_of::<Float>(), 24);
        assert_eq!(size_of::<String>(), 40);

        assert_eq!(size_of::<Array>(), 40);
        assert_eq!(size_of::<ByteArray>(), 40);

        // Permanent objects
        assert_eq!(size_of::<Method>(), 80);
        assert_eq!(size_of::<Class>(), 56);
    }

    #[test]
    fn test_type_alignments() {
        assert_eq!(align_of::<Header>(), ALIGNMENT);
        assert_eq!(align_of::<Object>(), ALIGNMENT);
        assert_eq!(align_of::<Int>(), ALIGNMENT);
        assert_eq!(align_of::<Float>(), ALIGNMENT);
        assert_eq!(align_of::<String>(), ALIGNMENT);
        assert_eq!(align_of::<Array>(), ALIGNMENT);
        assert_eq!(align_of::<ByteArray>(), ALIGNMENT);
        assert_eq!(align_of::<Method>(), ALIGNMENT);
        assert_eq!(align_of::<Class>(), ALIGNMENT);
    }

    #[test]
    fn test_pointer_with_mask() {
        let ptr = Pointer::with_mask(0x4 as _, 0b10);

        assert_eq!(ptr.as_ptr() as usize, 0x6);
    }

    #[test]
    fn test_pointer_integer_tagging() {
        unsafe {
            assert_eq!(Pointer::int(3).as_int(), 3);
            assert_eq!(Pointer::int(0).as_int(), 0);
            assert_eq!(Pointer::int(-3).as_int(), -3);

            assert!(Pointer::int(3).is_tagged_int());
        }
    }

    #[test]
    fn test_pointer_is_permanent_object() {
        let space = PermanentSpace::new(0, 0, Default::default());
        let ptr = Float::alloc(space.float_class(), 2.4);

        assert!(ptr.as_permanent().is_permanent());
        assert!(!ptr.is_permanent());

        unsafe { ptr.free() };
    }

    #[test]
    fn test_pointer_as_ref() {
        let space = PermanentSpace::new(0, 0, Default::default());
        let ptr = Float::alloc(space.float_class(), 2.4);

        assert!(!ptr.mask_is_set(REF_MASK));
        assert!(ptr.as_ref().mask_is_set(REF_MASK));

        unsafe { ptr.free() };
    }

    #[test]
    fn test_pointer_is_local_heap_object() {
        let space = PermanentSpace::new(0, 0, Default::default());
        let float = Float::alloc(space.float_class(), 8.0);

        assert!(!Pointer::int(42).is_local_heap_object());
        assert!(!Pointer::int(42).is_local_heap_object());
        assert!(!float.as_permanent().is_local_heap_object());
        assert!(!Pointer::true_singleton().is_local_heap_object());
        assert!(!Pointer::false_singleton().is_local_heap_object());
        assert!(float.is_local_heap_object());

        unsafe { float.free() };
    }

    #[test]
    fn test_pointer_get() {
        let space = PermanentSpace::new(0, 0, Default::default());
        let ptr = Int::alloc(space.int_class(), MAX_INTEGER + 1);

        unsafe {
            assert_eq!(ptr.get::<Int>().value, MAX_INTEGER + 1);
            ptr.free();
        }
    }

    #[test]
    fn test_pointer_get_mut() {
        let space = PermanentSpace::new(0, 0, Default::default());
        let ptr = Int::alloc(space.int_class(), MAX_INTEGER + 1);

        unsafe {
            ptr.get_mut::<Int>().value = MAX_INTEGER;

            assert_eq!(ptr.get::<Int>().value, MAX_INTEGER);
            ptr.free();
        }
    }

    #[test]
    fn test_pointer_drop_boxed() {
        unsafe {
            let ptr = Pointer::boxed(42_u64);

            // This is just a smoke test to make sure dropping a Box doesn't
            // crash or leak.
            ptr.drop_boxed::<u64>();
        }
    }

    #[test]
    fn test_pointer_untagged_ptr() {
        assert_eq!(
            Pointer::new(0x8 as _).as_permanent().untagged_ptr(),
            0x8 as *mut u8
        );
    }

    #[test]
    fn test_pointer_true_singleton() {
        assert_eq!(Pointer::true_singleton().as_ptr() as usize, TRUE_ADDRESS);
    }

    #[test]
    fn test_pointer_false_singleton() {
        assert_eq!(Pointer::false_singleton().as_ptr() as usize, FALSE_ADDRESS);
    }

    #[test]
    fn test_pointer_nil_singleton() {
        assert_eq!(Pointer::nil_singleton().as_ptr() as usize, NIL_ADDRESS);
    }

    #[test]
    fn test_pointer_undefined_singleton() {
        assert_eq!(
            Pointer::undefined_singleton().as_ptr() as usize,
            UNDEFINED_ADDRESS
        );
    }

    #[test]
    fn test_pointer_is_regular() {
        let space = PermanentSpace::new(0, 0, Default::default());
        let ptr = Float::alloc(space.float_class(), 2.4);

        assert!(ptr.is_regular());
        assert!(!Pointer::true_singleton().is_regular());
        assert!(!Pointer::false_singleton().is_regular());
        assert!(!Pointer::nil_singleton().is_regular());
        assert!(!Pointer::undefined_singleton().is_regular());
        assert!(!Pointer::int(42).is_regular());

        unsafe { ptr.free() };
    }

    #[test]
    fn test_pointer_is_tagged_int() {
        assert!(!Pointer::true_singleton().is_tagged_int());
        assert!(!Pointer::false_singleton().is_tagged_int());
        assert!(!Pointer::nil_singleton().is_tagged_int());
        assert!(!Pointer::undefined_singleton().is_tagged_int());
        assert!(Pointer::int(42).is_tagged_int());
        assert!(Pointer::int(42).is_tagged_int());
    }

    #[test]
    fn test_pointer_is_boolean() {
        assert!(Pointer::true_singleton().is_boolean());
        assert!(Pointer::false_singleton().is_boolean());
        assert!(!Pointer::nil_singleton().is_boolean());
        assert!(!Pointer::int(24).is_boolean());
        assert!(!Pointer::int(24).is_boolean());
    }

    #[test]
    fn test_class_new() {
        let class = Class::alloc("A".to_string(), 0, 24);

        assert_eq!(class.method_slots, 0);
        assert_eq!(class.instance_size, 24);

        Class::drop(class);
    }

    #[test]
    fn test_class_new_object() {
        let class = Class::object("A".to_string(), 1, 0);

        assert_eq!(class.method_slots, 0);
        assert_eq!(class.instance_size, 24);

        Class::drop(class);
    }

    #[test]
    fn test_class_new_process() {
        let class = Class::process("A".to_string(), 1, 0);

        assert_eq!(class.method_slots, 0);

        Class::drop(class);
    }

    #[test]
    fn test_class_of() {
        let space = PermanentSpace::new(0, 0, Default::default());
        let string = String::alloc(space.string_class(), "A".to_string());
        let perm_string = space.allocate_string("B".to_string());

        assert!(Class::of(&space, Pointer::int(42)) == space.int_class());
        assert!(
            Class::of(&space, Pointer::true_singleton())
                == space.boolean_class()
        );
        assert!(
            Class::of(&space, Pointer::false_singleton())
                == space.boolean_class()
        );
        assert!(
            Class::of(&space, Pointer::nil_singleton()) == space.nil_class()
        );
        assert!(Class::of(&space, string) == space.string_class());
        assert!(Class::of(&space, perm_string) == space.string_class());

        unsafe {
            String::drop_and_deallocate(string);
        };
    }

    #[test]
    fn test_class_methods() {
        let mod_class =
            OwnedClass::new(Class::object("foo_mod".to_string(), 0, 2));
        let module = empty_module(*mod_class);
        let foo = empty_method();
        let bar = empty_method();
        let index0 = MethodIndex::new(0);
        let index1 = MethodIndex::new(1);

        unsafe {
            module.header.class.set_method(index0, foo);
            module.header.class.set_method(index1, bar);

            assert_eq!(
                module.header.class.get_method(index0).as_ptr(),
                foo.as_ptr()
            );

            assert_eq!(
                module.header.class.get_method(index1).as_ptr(),
                bar.as_ptr()
            );
        }
    }

    #[test]
    fn test_int_read() {
        let class = OwnedClass::new(Class::object("Int".to_string(), 0, 0));
        let tagged = Int::alloc(*class, 42);
        let max = Int::alloc(*class, i64::MAX);
        let min = Int::alloc(*class, i64::MIN);

        assert_eq!(unsafe { Int::read(tagged) }, 42);
        assert_eq!(unsafe { Int::read(max) }, i64::MAX);
        assert_eq!(unsafe { Int::read(min) }, i64::MIN);

        unsafe {
            min.free();
            max.free();
        }
    }

    #[test]
    fn test_int_read_u64() {
        let class = OwnedClass::new(Class::object("Int".to_string(), 0, 0));
        let tagged = Int::alloc(*class, 42);
        let max = Int::alloc(*class, i64::MAX);
        let min = Int::alloc(*class, i64::MIN);

        assert_eq!(unsafe { Int::read_u64(tagged) }, 42);
        assert_eq!(unsafe { Int::read_u64(max) }, i64::MAX as u64);
        assert_eq!(unsafe { Int::read_u64(min) }, 0);

        unsafe {
            min.free();
            max.free();
        }
    }
}
