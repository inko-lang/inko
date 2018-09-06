// This lint is disabled here as not passing pointers by reference could
// potentially result in forwarding pointers not working properly.
#![cfg_attr(feature = "cargo-clippy", allow(clippy::trivially_copy_pass_by_ref))]

use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use std::fs;
use std::hash::{Hash, Hasher as HasherTrait};
use std::i32;
use std::i64;
use std::u32;
use std::u8;
use std::usize;

use binding::RcBinding;
use block::Block;
use hasher::Hasher;
use immix::bitmap::Bitmap;
use immix::block;
use immix::bucket::{MAILBOX, MATURE, PERMANENT};
use immix::local_allocator::YOUNG_MAX_AGE;
use object::{Object, ObjectStatus, FORWARDED_BIT};
use object_value::ObjectValue;
use process::RcProcess;
use tagged_pointer::TaggedPointer;
use vm::state::RcState;

/// Performs a write to an object and tracks it in the write barrier.
macro_rules! write_object {
    ($receiver:expr, $process:expr, $action:expr, $value:expr) => {{
        let track = !$receiver.get().has_attributes();
        let pointer = *$receiver;

        $action;

        $process.write_barrier(pointer, $value);

        if track && $receiver.is_finalizable() {
            $receiver.mark_for_finalization();
        }
    }};
}

/// Defines a method for getting the value of an object as a given type.
macro_rules! def_value_getter {
    ($name: ident, $getter: ident, $as_type: ident, $ok_type: ty) => (
        pub fn $name(&self) -> Result<$ok_type, String> {
            if self.is_tagged_integer() {
                Err(format!("ObjectPointer::{}() called on a tagged integer",
                            stringify!($as_type)))
            } else {
                self.$getter().value.$as_type()
            }
        }
    )
}

/// The minimum integer value that can be stored as a tagged integer.
pub const MIN_INTEGER: i64 = i64::MIN >> 1;

/// The maximum integer value that can be stored as a tagged integer.
pub const MAX_INTEGER: i64 = i64::MAX >> 1;

pub type RawObjectPointer = *mut Object;

/// A pointer to an object managed by the GC.
#[derive(Clone, Copy)]
pub struct ObjectPointer {
    /// The underlying tagged pointer. This pointer can have the following last
    /// two bits set:
    ///
    ///     00: the pointer is a regular pointer
    ///     01: the pointer is a tagged integer
    ///     10: the pointer is a forwarding pointer
    pub raw: TaggedPointer<Object>,
}

unsafe impl Send for ObjectPointer {}
unsafe impl Sync for ObjectPointer {}

/// A pointer to a object pointer. This wrapper is necessary to allow sharing
/// *const ObjectPointer pointers between threads.
pub struct ObjectPointerPointer {
    pub raw: *const ObjectPointer,
}

unsafe impl Send for ObjectPointerPointer {}
unsafe impl Sync for ObjectPointerPointer {}

/// The bit to set for tagged integers.
pub const INTEGER_BIT: usize = 0;

/// Returns the BlockHeader of the given pointer.
fn block_header_of<'a>(pointer: RawObjectPointer) -> &'a block::BlockHeader {
    let addr = (pointer as isize & block::OBJECT_BITMAP_MASK) as usize;

    unsafe {
        let ptr = addr as *mut block::BlockHeader;

        &*ptr
    }
}

impl ObjectPointer {
    pub fn new(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::new(pointer),
        }
    }

    /// Creates a new tagged integer.
    pub fn integer(value: i64) -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::with_bit(
                (value << 1) as RawObjectPointer,
                INTEGER_BIT,
            ),
        }
    }

    pub fn byte(value: u8) -> ObjectPointer {
        Self::integer(i64::from(value))
    }

    /// Returns `true` if the given unsigned integer is too large for a tagged
    /// pointer.
    pub fn unsigned_integer_too_large(value: u64) -> bool {
        value > MAX_INTEGER as u64
    }

    /// Returns `true` if the given unsigned integer should be allocated as a
    /// big integer.
    pub fn unsigned_integer_as_big_integer(value: u64) -> bool {
        value > i64::MAX as u64
    }

    /// Returns `true` if the given value is too large for a tagged pointer.
    pub fn integer_too_large(value: i64) -> bool {
        value < MIN_INTEGER || value > MAX_INTEGER
    }

    /// Creates a new null pointer.
    pub fn null() -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::null(),
        }
    }

    /// Returns true if the current pointer points to a forwarded object.
    #[inline(always)]
    pub fn is_forwarded(&self) -> bool {
        self.get().is_forwarded()
    }

    /// Returns the status of the object.
    #[inline(always)]
    pub fn status(&mut self) -> ObjectStatus {
        if self.is_forwarded() {
            return ObjectStatus::Resolve;
        }

        let block = self.block();

        if block.is_fragmented() && self.get_mut().mark_for_forward() {
            return ObjectStatus::Evacuate;
        }

        if block.bucket().unwrap().promote && self.get_mut().mark_for_forward()
        {
            return ObjectStatus::Promote;
        }

        ObjectStatus::OK
    }

    /// Replaces the current pointer with a pointer to the forwarded object.
    pub fn resolve_forwarding_pointer(&mut self) {
        let raw_attrs = self.get().attributes;

        // It's possible that between a previous check and calling this method
        // the pointer has already been resolved. In this case we should _not_
        // try to resolve anything as we'd end up storing the address to the
        // target objects' _prototype_, and not the target object itself.
        if raw_attrs.bit_is_set(FORWARDED_BIT) {
            self.raw =
                TaggedPointer::new(raw_attrs.untagged() as RawObjectPointer);
        }
    }

    /// Returns an immutable reference to the Object.
    #[inline(always)]
    pub fn get(&self) -> &Object {
        self.raw
            .as_ref()
            .expect("ObjectPointer::get() called on a NULL pointer")
    }

    /// Returns a mutable reference to the Object.
    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    pub fn get_mut(&self) -> &mut Object {
        self.raw
            .as_mut()
            .expect("ObjectPointer::get_mut() called on a NULL pointer")
    }

    /// Returns true if the current pointer is a null pointer.
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.raw.raw as usize == 0x0
    }

    /// Returns true if the current pointer points to a permanent object.
    pub fn is_permanent(&self) -> bool {
        self.is_tagged_integer()
            || self.block().bucket().unwrap().age == PERMANENT
    }

    /// Returns true if the current pointer points to a mature object.
    pub fn is_mature(&self) -> bool {
        !self.is_tagged_integer()
            && self.block().bucket().unwrap().age == MATURE
    }

    /// Returns true if the current pointer points to a mailbox object.
    pub fn is_mailbox(&self) -> bool {
        !self.is_tagged_integer()
            && self.block().bucket().unwrap().age == MAILBOX
    }

    /// Returns true if the current pointer points to a young object.
    pub fn is_young(&self) -> bool {
        !self.is_tagged_integer()
            && self.block().bucket().unwrap().age <= YOUNG_MAX_AGE
    }

    pub fn mark_for_finalization(&self) {
        let block = self.block_mut();
        let index = block.object_index_of_pointer(self.raw.untagged());

        block.finalize_bitmap.set(index);
    }

    pub fn unmark_for_finalization(&self) {
        let block = self.block_mut();
        let index = block.object_index_of_pointer(self.raw.untagged());

        block.finalize_bitmap.unset(index);
    }

    /// Marks the current object and its line.
    ///
    /// As this method is called often during collection, this method refers to
    /// `self.raw` only once and re-uses the pointer. This ensures there are no
    /// race conditions when determining the object/line indexes, and reduces
    /// the overhead of having to call `self.raw.untagged()` multiple times.
    pub fn mark(&self) {
        let pointer = self.raw.untagged();
        let header = block_header_of(pointer);
        let block = header.block_mut();

        let object_index = block.object_index_of_pointer(pointer);
        let line_index = block.line_index_of_pointer(pointer);

        block.marked_objects_bitmap.set(object_index);
        block.used_lines_bitmap.set(line_index);
    }

    /// Returns true if the current object is marked.
    ///
    /// This method *must not* use any methods that also call
    /// `self.raw.untagged()` as doing so will lead to race conditions producing
    /// incorrect object/line indexes. This can happen when one tried checks if
    /// an object is marked while another thread is updating the pointer's
    /// address (e.g. after evacuating the underlying object).
    pub fn is_marked(&self) -> bool {
        if self.is_tagged_integer() {
            return true;
        }

        let pointer = self.raw.untagged();
        let header = block_header_of(pointer);
        let block = header.block_mut();
        let index = block.object_index_of_pointer(pointer);

        block.marked_objects_bitmap.is_set(index)
    }

    /// Returns a mutable reference to the block this pointer belongs to.
    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    pub fn block_mut(&self) -> &mut block::Block {
        self.block_header().block_mut()
    }

    /// Returns an immutable reference to the block this pointer belongs to.
    #[inline(always)]
    pub fn block(&self) -> &block::Block {
        self.block_header().block()
    }

    /// Returns an immutable reference to the header of the block this pointer
    /// belongs to.
    #[inline(always)]
    pub fn block_header(&self) -> &block::BlockHeader {
        block_header_of(self.raw.untagged())
    }

    /// Returns true if the object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        !self.is_tagged_integer() && self.get().is_finalizable()
    }

    /// Adds an attribute to the object this pointer points to.
    pub fn add_attribute(
        &self,
        process: &RcProcess,
        name: ObjectPointer,
        attr: ObjectPointer,
    ) {
        write_object!(
            self,
            process,
            self.get_mut().add_attribute(name, attr),
            attr
        );
    }

    /// Looks up an attribute.
    pub fn lookup_attribute(
        &self,
        state: &RcState,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            state.integer_prototype.get().lookup_attribute(name)
        } else {
            self.get().lookup_attribute(name)
        }
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(
        &self,
        state: &RcState,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            state.integer_prototype.get().lookup_attribute_in_self(name)
        } else {
            self.get().lookup_attribute_in_self(name)
        }
    }

    pub fn attributes(&self) -> Vec<ObjectPointer> {
        if self.is_tagged_integer() {
            Vec::new()
        } else {
            self.get().attributes()
        }
    }

    pub fn attribute_names(&self) -> Vec<ObjectPointer> {
        if self.is_tagged_integer() {
            Vec::new()
        } else {
            self.get().attribute_names()
        }
    }

    pub fn set_prototype(&self, proto: ObjectPointer) {
        self.get_mut().set_prototype(proto);
    }

    pub fn prototype(&self, state: &RcState) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            Some(state.integer_prototype)
        } else {
            self.get().prototype()
        }
    }

    pub fn is_kind_of(&self, state: &RcState, other: ObjectPointer) -> bool {
        let mut prototype = self.prototype(state);

        while let Some(proto) = prototype {
            if proto == other {
                return true;
            }

            prototype = proto.prototype(state);
        }

        false
    }

    /// Returns a pointer to this pointer.
    pub fn pointer(&self) -> ObjectPointerPointer {
        ObjectPointerPointer::new(self)
    }

    pub fn is_tagged_integer(&self) -> bool {
        self.raw.bit_is_set(INTEGER_BIT)
    }

    pub fn is_string(&self) -> bool {
        if self.is_tagged_integer() {
            false
        } else {
            self.get().value.is_string()
        }
    }

    pub fn is_interned_string(&self) -> bool {
        if self.is_tagged_integer() {
            false
        } else {
            self.get().value.is_interned_string()
        }
    }

    pub fn is_integer(&self) -> bool {
        self.is_tagged_integer() || self.get().value.is_integer()
    }

    pub fn is_bigint(&self) -> bool {
        if self.is_integer() {
            false
        } else {
            self.get().value.is_bigint()
        }
    }

    pub fn is_zero_integer(&self) -> bool {
        if self.is_integer() {
            self.integer_value().unwrap().is_zero()
        } else if self.is_bigint() {
            self.bigint_value().unwrap().is_zero()
        } else {
            false
        }
    }

    pub fn is_in_u32_range(&self) -> bool {
        if let Ok(integer) = self.integer_value() {
            integer >= i64::from(u32::MIN) && integer <= i64::from(u32::MAX)
        } else {
            false
        }
    }

    pub fn is_in_i32_range(&self) -> bool {
        if let Ok(integer) = self.integer_value() {
            integer >= i64::from(i32::MIN) && integer <= i64::from(i32::MAX)
        } else {
            false
        }
    }

    pub fn is_immutable(&self) -> bool {
        self.is_tagged_integer() || self.get().value.is_immutable()
    }

    pub fn integer_value(&self) -> Result<i64, String> {
        if self.is_tagged_integer() {
            Ok(self.raw.raw as i64 >> 1)
        } else if let Ok(num) = self.get().value.as_integer() {
            Ok(num)
        } else {
            Err(
                "ObjectPointer::integer_value() called on a non integer object"
                    .to_string(),
            )
        }
    }

    pub fn integer_to_usize(&self) -> Result<usize, String> {
        let int_val = self.integer_value()?;

        if int_val < 0 || int_val as u64 > usize::MAX as u64 {
            Err(format!(
                "{} is too big to convert to an unsigned integer",
                int_val
            ))
        } else {
            Ok(int_val as usize)
        }
    }

    pub fn bigint_to_usize(&self) -> Result<usize, String> {
        let int_val = self.bigint_value()?;

        int_val.to_usize().ok_or_else(|| {
            format!("{} is too big to convert to an unsigned integer", int_val)
        })
    }

    pub fn usize_value(&self) -> Result<usize, String> {
        if self.is_bigint() {
            self.bigint_to_usize()
        } else {
            self.integer_to_usize()
        }
    }

    pub fn u8_value(&self) -> Result<u8, String> {
        if self.is_bigint() {
            Err(format!(
                "{} is too big to be converted to a byte",
                self.bigint_value()?
            ))
        } else {
            let int_val = self.integer_value()?;

            if int_val < 0 || int_val > i64::from(u8::MAX) {
                Err(format!("{} is too big to convert to a byte", int_val))
            } else {
                Ok(int_val as u8)
            }
        }
    }

    pub fn i32_value(&self) -> Result<i32, String> {
        let int_val = self.integer_value()?;

        if int_val < i64::from(i32::MIN) || int_val > i64::from(i32::MAX) {
            Err(format!("{} is not a valid exit status code", int_val))
        } else {
            Ok(int_val as i32)
        }
    }

    pub fn hash_object(&self, hasher: &mut Hasher) -> Result<(), String> {
        if self.is_tagged_integer() {
            hasher.write_integer(self.integer_value()?);
        } else {
            let value_ref = self.get();

            match value_ref.value {
                ObjectValue::Float(val) => hasher.write_float(val),
                ObjectValue::Integer(val) => hasher.write_integer(val),
                ObjectValue::BigInt(ref val) => hasher.write_bigint(val),
                ObjectValue::String(ref val) => hasher.write_string(val),
                ObjectValue::InternedString(ref val) => {
                    hasher.write_string(val)
                }
                _ => {
                    if !self.is_permanent() {
                        return Err(
                            "the provided object can not be hashed".to_string()
                        );
                    }

                    hasher.write_unsigned_integer(self.raw.untagged() as usize);
                }
            };
        }

        Ok(())
    }

    def_value_getter!(float_value, get, as_float, f64);
    def_value_getter!(string_value, get, as_string, &String);

    def_value_getter!(array_value, get, as_array, &Vec<ObjectPointer>);
    def_value_getter!(
        array_value_mut,
        get_mut,
        as_array_mut,
        &mut Vec<ObjectPointer>
    );

    def_value_getter!(file_value, get, as_file, &fs::File);
    def_value_getter!(file_value_mut, get_mut, as_file_mut, &mut fs::File);

    def_value_getter!(block_value, get, as_block, &Box<Block>);
    def_value_getter!(binding_value, get, as_binding, RcBinding);
    def_value_getter!(bigint_value, get, as_bigint, &BigInt);
    def_value_getter!(hasher_value_mut, get_mut, as_hasher_mut, &mut Hasher);

    def_value_getter!(byte_array_value, get, as_byte_array, &Vec<u8>);
    def_value_getter!(
        byte_array_value_mut,
        get_mut,
        as_byte_array_mut,
        &mut Vec<u8>
    );

    /// Atomically loads the underlying pointer, returning a new ObjectPointer.
    pub fn atomic_load(&self) -> Self {
        ObjectPointer {
            raw: TaggedPointer::new(self.raw.atomic_load()),
        }
    }
}

impl ObjectPointerPointer {
    #[cfg_attr(
        feature = "cargo-clippy",
        allow(clippy::trivially_copy_pass_by_ref)
    )]
    pub fn new(pointer: &ObjectPointer) -> ObjectPointerPointer {
        ObjectPointerPointer {
            raw: pointer as *const ObjectPointer,
        }
    }

    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    pub fn get_mut(&self) -> &mut ObjectPointer {
        unsafe { &mut *(self.raw as *mut ObjectPointer) }
    }

    #[inline(always)]
    pub fn get(&self) -> &ObjectPointer {
        unsafe { &*self.raw }
    }
}

impl PartialEq for ObjectPointer {
    fn eq(&self, other: &ObjectPointer) -> bool {
        self.raw == other.raw
    }
}

impl Eq for ObjectPointer {}

impl Hash for ObjectPointer {
    fn hash<H: HasherTrait>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl PartialEq for ObjectPointerPointer {
    fn eq(&self, other: &ObjectPointerPointer) -> bool {
        self.raw == other.raw
    }
}

impl Eq for ObjectPointerPointer {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::i128;

    use config::Config;
    use immix::bitmap::Bitmap;
    use immix::block::Block;
    use immix::bucket::{Bucket, MAILBOX, MATURE, PERMANENT};
    use immix::global_allocator::GlobalAllocator;
    use immix::local_allocator::LocalAllocator;
    use object::{Object, ObjectStatus};
    use object_value::{self, ObjectValue};
    use vm::state::State;

    fn fake_raw_pointer() -> RawObjectPointer {
        0x4 as RawObjectPointer
    }

    fn object_pointer_for(object: &Object) -> ObjectPointer {
        ObjectPointer::new(object as *const Object as RawObjectPointer)
    }

    fn local_allocator() -> LocalAllocator {
        LocalAllocator::new(GlobalAllocator::new(), &Config::new())
    }

    fn buckets_for_all_ages() -> (Bucket, Bucket, Bucket, Bucket) {
        let young = Bucket::with_age(0);
        let mature = Bucket::with_age(MATURE);
        let mailbox = Bucket::with_age(MAILBOX);
        let permanent = Bucket::with_age(PERMANENT);

        (young, mature, mailbox, permanent)
    }

    fn allocate_in_bucket(bucket: &mut Bucket) -> ObjectPointer {
        if bucket.blocks.is_empty() {
            bucket.add_block(Block::new());
        }

        let raw_pointer =
            bucket.current_block().unwrap().request_pointer().unwrap();

        Object::new(ObjectValue::None).write_to(raw_pointer)
    }

    #[test]
    fn test_object_pointer_new() {
        let pointer = ObjectPointer::new(fake_raw_pointer());

        assert_eq!(pointer.raw.raw as usize, 0x4);
    }

    #[test]
    fn test_object_pointer_null() {
        let pointer = ObjectPointer::null();

        assert_eq!(pointer.raw.raw as usize, 0x0);
    }

    #[test]
    fn test_byte() {
        let pointer = ObjectPointer::byte(5);

        assert_eq!(pointer.integer_value().unwrap(), 5);
    }

    #[test]
    fn test_object_pointer_is_forwarded_with_regular_pointer() {
        let object = Object::new(ObjectValue::None);
        let pointer = object_pointer_for(&object);

        assert_eq!(pointer.is_forwarded(), false);
    }

    #[test]
    fn test_object_pointer_is_forwarded_with_forwarding_pointer() {
        let mut source = Object::new(ObjectValue::None);
        let target = Object::new(ObjectValue::None);
        let target_ptr = object_pointer_for(&target);

        source.forward_to(target_ptr);

        let source_ptr = object_pointer_for(&source);

        assert_eq!(source_ptr.is_forwarded(), true);
    }

    #[test]
    fn test_object_pointer_status() {
        let mut allocator = local_allocator();
        let mut pointer = allocator.allocate_empty();

        assert_eq!(pointer.status(), ObjectStatus::OK);
    }

    #[test]
    fn test_object_pointer_status_promote() {
        let mut allocator = local_allocator();
        let mut pointer = allocator.allocate_empty();

        pointer.block_mut().bucket_mut().unwrap().promote = true;

        assert_eq!(pointer.status(), ObjectStatus::Promote);

        // The first status check will acquire the "right" to promote the
        // object. All other status checks will simply return OK.
        assert_eq!(pointer.status(), ObjectStatus::OK);
    }

    #[test]
    fn test_object_pointer_status_fragmented() {
        let mut allocator = local_allocator();
        let mut pointer = allocator.allocate_empty();
        let pointer2 = allocator.allocate_empty();

        pointer.block_mut().set_fragmented();

        assert_eq!(pointer.status(), ObjectStatus::Evacuate);

        // The first status check will acquire the "right" to promote the
        // object. All other status checks will simply return OK.
        assert_eq!(pointer.status(), ObjectStatus::OK);

        pointer.get_mut().forward_to(pointer2);

        assert_eq!(pointer.status(), ObjectStatus::Resolve);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer() {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Float(2.0));

        object.forward_to(proto_pointer);

        let mut pointer = object_pointer_for(&object);

        pointer.resolve_forwarding_pointer();

        assert!(pointer == proto_pointer);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer_concurrently() {
        // The object to forward to.
        let mut target_object = Object::new(ObjectValue::None);
        let target_proto = Object::new(ObjectValue::None);
        let target_pointer = object_pointer_for(&target_object);

        // For this test the target object must have a prototype. This ensures
        // resolving the pointer doesn't return early due to there not being a
        // prototype.
        target_object.set_prototype(object_pointer_for(&target_proto));

        // The object that is being forwarded.
        let mut forwarded_object = Object::new(ObjectValue::None);

        forwarded_object.forward_to(target_pointer);

        let forwarded_pointer = object_pointer_for(&forwarded_object);

        let ptr_ptr1 = forwarded_pointer.pointer();
        let ptr_ptr2 = forwarded_pointer.pointer();

        // "Simulate" two threads concurrently resolving the same pointer.
        let ptr1 = ptr_ptr1.get_mut();
        let ptr2 = ptr_ptr2.get_mut();

        ptr1.resolve_forwarding_pointer();
        ptr2.resolve_forwarding_pointer();

        assert!(*ptr1 == target_pointer);
        assert!(*ptr2 == target_pointer);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer_in_vector() {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Float(2.0));

        object.forward_to(proto_pointer);

        let mut pointers = vec![object_pointer_for(&object)];

        pointers.get_mut(0).unwrap().resolve_forwarding_pointer();

        assert!(pointers[0] == proto_pointer);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer_in_vector_with_pointer_pointers(
) {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Float(2.0));

        object.forward_to(proto_pointer);

        let mut pointers = vec![object_pointer_for(&object)];
        let mut pointer_pointers = vec![&mut pointers[0] as *mut ObjectPointer];

        let ptr_ref = unsafe { &mut *pointer_pointers[0] };

        ptr_ref.resolve_forwarding_pointer();

        assert!(pointers[0] == proto_pointer);
    }

    #[test]
    fn test_object_pointer_get_get_mut() {
        let object = Object::new(ObjectValue::Float(2.0));
        let pointer = object_pointer_for(&object);

        // Object doesn't implement PartialEq/Eq so we can't compare references,
        // thus we'll just test if we get something somewhat correct-ish.
        assert!(pointer.get().value.is_float());
        assert!(pointer.get_mut().value.is_float());
    }

    #[test]
    fn test_object_pointer_is_null_with_null_pointer() {
        let pointer = ObjectPointer::null();

        assert!(pointer.is_null());
    }

    #[test]
    fn test_object_pointer_is_null_with_regular_pointer() {
        let pointer = ObjectPointer::new(fake_raw_pointer());

        assert_eq!(pointer.is_null(), false);
    }

    #[test]
    fn test_object_pointer_is_permanent() {
        let (mut young, mut mature, mut mailbox, mut permanent) =
            buckets_for_all_ages();

        let young_ptr = allocate_in_bucket(&mut young);
        let mature_ptr = allocate_in_bucket(&mut mature);
        let mailbox_ptr = allocate_in_bucket(&mut mailbox);
        let permanent_ptr = allocate_in_bucket(&mut permanent);

        assert_eq!(young_ptr.is_permanent(), false);
        assert_eq!(mature_ptr.is_permanent(), false);
        assert_eq!(mailbox_ptr.is_permanent(), false);
        assert_eq!(permanent_ptr.is_permanent(), true);
    }

    #[test]
    fn test_object_pointer_is_young() {
        let (mut young, mut mature, mut mailbox, mut permanent) =
            buckets_for_all_ages();

        let young_ptr = allocate_in_bucket(&mut young);
        let mature_ptr = allocate_in_bucket(&mut mature);
        let mailbox_ptr = allocate_in_bucket(&mut mailbox);
        let permanent_ptr = allocate_in_bucket(&mut permanent);

        assert_eq!(young_ptr.is_young(), true);
        assert_eq!(mature_ptr.is_young(), false);
        assert_eq!(mailbox_ptr.is_young(), false);
        assert_eq!(permanent_ptr.is_young(), false);
    }

    #[test]
    fn test_object_pointer_is_mature() {
        let (mut young, mut mature, mut mailbox, mut permanent) =
            buckets_for_all_ages();

        let young_ptr = allocate_in_bucket(&mut young);
        let mature_ptr = allocate_in_bucket(&mut mature);
        let mailbox_ptr = allocate_in_bucket(&mut mailbox);
        let permanent_ptr = allocate_in_bucket(&mut permanent);

        assert_eq!(young_ptr.is_mature(), false);
        assert_eq!(mature_ptr.is_mature(), true);
        assert_eq!(mailbox_ptr.is_mature(), false);
        assert_eq!(permanent_ptr.is_mature(), false);
    }

    #[test]
    fn test_object_pointer_is_mailbox() {
        let (mut young, mut mature, mut mailbox, mut permanent) =
            buckets_for_all_ages();

        let young_ptr = allocate_in_bucket(&mut young);
        let mature_ptr = allocate_in_bucket(&mut mature);
        let mailbox_ptr = allocate_in_bucket(&mut mailbox);
        let permanent_ptr = allocate_in_bucket(&mut permanent);

        assert_eq!(young_ptr.is_mailbox(), false);
        assert_eq!(mature_ptr.is_mailbox(), false);
        assert_eq!(mailbox_ptr.is_mailbox(), true);
        assert_eq!(permanent_ptr.is_mailbox(), false);
    }

    #[test]
    fn test_object_pointer_is_marked_with_unmarked_object() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        assert_eq!(pointer.is_marked(), false);
    }

    #[test]
    fn test_object_pointer_is_marked_with_marked_object() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.mark();

        assert!(pointer.is_marked());
    }

    #[test]
    fn test_object_pointer_mark() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.mark();

        assert!(pointer.block().marked_objects_bitmap.is_set(4));
        assert!(pointer.block().used_lines_bitmap.is_set(1));
    }

    #[test]
    fn test_object_pointer_block_mut() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.block_mut();
    }

    #[test]
    fn test_object_pointer_block() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.block();
    }

    #[test]
    fn test_object_pointer_block_header() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.block_header();
    }

    #[test]
    fn test_object_pointer_pointer() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        let raw_pointer = pointer.pointer();

        assert!(*raw_pointer.get() == pointer);

        // Using the raw pointer for any updates should result in the
        // ObjectPointer being updated properly.
        let reference = raw_pointer.get_mut();

        reference.raw.set_bit(0);

        assert!(pointer.raw.bit_is_set(0));
    }

    #[test]
    fn test_object_pointer_eq() {
        let mut allocator = local_allocator();
        let pointer1 = allocator.allocate_empty();
        let pointer2 = allocator.allocate_empty();

        assert!(pointer1 == pointer1);
        assert!(pointer1 != pointer2);
    }

    #[test]
    fn test_object_pointer_hashing() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();
        let mut set = HashSet::new();

        set.insert(pointer);

        assert!(set.contains(&pointer));
    }

    #[test]
    fn test_object_pointer_integer_too_large() {
        assert_eq!(ObjectPointer::integer_too_large(i64::MAX), true);
        assert_eq!(ObjectPointer::integer_too_large(MAX_INTEGER), false);
        assert_eq!(ObjectPointer::integer_too_large(5), false);
    }

    #[test]
    fn test_object_pointer_integer_value() {
        for i in 1..10 {
            assert_eq!(ObjectPointer::integer(i).integer_value().unwrap(), i);
        }
    }

    #[test]
    fn test_object_pointer_maximum_value() {
        let valid = ObjectPointer::integer(MAX_INTEGER);
        let invalid = ObjectPointer::integer(MAX_INTEGER + 1);

        assert_eq!(valid.integer_value().unwrap(), MAX_INTEGER);
        assert_eq!(invalid.integer_value().unwrap(), MIN_INTEGER);
    }

    #[test]
    fn test_object_pointer_minimum_value() {
        let valid = ObjectPointer::integer(MIN_INTEGER);
        let invalid = ObjectPointer::integer(MIN_INTEGER - 1);

        assert_eq!(valid.integer_value().unwrap(), MIN_INTEGER);
        assert_eq!(invalid.integer_value().unwrap(), MAX_INTEGER);
    }

    #[test]
    fn test_object_pointer_lookup_attribute_with_integer() {
        let state = State::new(Config::new(), &[]);
        let ptr = ObjectPointer::integer(5);
        let name = state.intern_owned("foo".to_string());
        let method = state.permanent_allocator.lock().allocate_empty();

        state
            .integer_prototype
            .get_mut()
            .add_attribute(name, method);

        state.integer_prototype.mark_for_finalization();

        assert!(ptr.lookup_attribute(&state, name).unwrap() == method);
    }

    #[test]
    fn test_object_pointer_lookup_attribute_in_self_with_integer() {
        let state = State::new(Config::new(), &[]);
        let ptr = ObjectPointer::integer(5);
        let name = state.intern_owned("foo".to_string());
        let method = state.permanent_allocator.lock().allocate_empty();

        state
            .integer_prototype
            .get_mut()
            .add_attribute(name, method);

        state.integer_prototype.mark_for_finalization();

        assert!(ptr.lookup_attribute_in_self(&state, name).unwrap() == method);
    }

    #[test]
    fn test_object_pointer_lookup_attribute_with_integer_without_attribute() {
        let state = State::new(Config::new(), &[]);
        let ptr = ObjectPointer::integer(5);
        let name = state.intern_owned("foo".to_string());

        assert!(ptr.lookup_attribute(&state, name).is_none());
    }

    #[test]
    fn test_object_pointer_lookup_attribute_with_object() {
        let state = State::new(Config::new(), &[]);
        let ptr = state.permanent_allocator.lock().allocate_empty();
        let name = state.intern_owned("foo".to_string());
        let value = state.permanent_allocator.lock().allocate_empty();

        ptr.get_mut().add_attribute(name, value);
        ptr.mark_for_finalization();

        assert!(ptr.lookup_attribute(&state, name).unwrap() == value);
    }

    #[test]
    fn test_object_pointer_pointer_get_mut() {
        let ptr = ObjectPointer::new(fake_raw_pointer());
        let ptr_ptr = ptr.pointer();

        ptr_ptr.get_mut().raw.raw = 0x5 as RawObjectPointer;

        assert_eq!(ptr.raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_pointer_pointer_eq() {
        let ptr = ObjectPointer::new(fake_raw_pointer());
        let ptr_ptr1 = ptr.pointer();
        let ptr_ptr2 = ptr.pointer();

        assert!(ptr_ptr1 == ptr_ptr2);
    }

    #[test]
    fn test_is_immutable() {
        let state = State::new(Config::new(), &[]);
        let name = state.intern_owned("foo".to_string());

        assert!(name.is_immutable());
        assert!(ObjectPointer::integer(5).is_immutable());
    }

    #[test]
    fn test_is_finalizable() {
        let mut allocator = local_allocator();
        let ptr =
            allocator.allocate_without_prototype(ObjectValue::Integer(10));

        assert!(ptr.is_finalizable());
        assert_eq!(ObjectPointer::integer(5).is_finalizable(), false);
    }

    #[test]
    fn test_usize_value() {
        let ptr = ObjectPointer::integer(5);

        assert_eq!(ptr.usize_value().unwrap(), 5);
    }

    #[test]
    fn test_u8_value() {
        let mut alloc = local_allocator();

        let valid = ObjectPointer::integer(5);
        let invalid = ObjectPointer::integer(300);
        let bigint = alloc.allocate_without_prototype(object_value::bigint(
            BigInt::from(5000),
        ));

        assert!(valid.u8_value().is_ok());
        assert!(invalid.u8_value().is_err());
        assert!(bigint.u8_value().is_err());

        assert_eq!(valid.u8_value().unwrap(), 5);
    }

    #[test]
    fn test_integer_to_usize() {
        assert_eq!(ObjectPointer::integer(5).integer_to_usize().unwrap(), 5);
        assert!(ObjectPointer::integer(-5).integer_to_usize().is_err());
    }

    #[test]
    fn test_bigint_to_usize() {
        let mut alloc = local_allocator();
        let small = alloc
            .allocate_without_prototype(object_value::bigint(BigInt::from(5)));

        let big = alloc.allocate_without_prototype(object_value::bigint(
            BigInt::from(i128::MAX),
        ));

        assert_eq!(small.bigint_to_usize().unwrap(), 5);
        assert!(big.bigint_to_usize().is_err());
    }

    #[test]
    fn test_i32_value() {
        let small = ObjectPointer::integer(5);
        let large = ObjectPointer::integer(i32::MAX as i64 + 1);

        assert_eq!(small.i32_value().unwrap(), 5);
        assert!(large.i32_value().is_err());
    }

    #[test]
    fn test_is_zero_integer_with_tagged_integers() {
        assert_eq!(ObjectPointer::integer(5).is_zero_integer(), false);
        assert!(ObjectPointer::integer(0).is_zero_integer());
    }

    #[test]
    fn test_is_zero_integer_with_heap_integers() {
        let mut alloc = local_allocator();
        let non_zero =
            alloc.allocate_without_prototype(object_value::integer(5));

        let zero = alloc.allocate_without_prototype(object_value::integer(0));

        assert_eq!(non_zero.is_zero_integer(), false);
        assert!(zero.is_zero_integer());
    }

    #[test]
    fn test_is_zero_integer_with_big_integers() {
        let mut alloc = local_allocator();
        let non_zero = alloc
            .allocate_without_prototype(object_value::bigint(BigInt::from(5)));

        let zero = alloc
            .allocate_without_prototype(object_value::bigint(BigInt::from(0)));

        assert_eq!(non_zero.is_zero_integer(), false);
        assert!(zero.is_zero_integer());
    }

    #[test]
    fn test_atomic_load() {
        let mut alloc = local_allocator();
        let pointer = alloc.allocate_empty();

        assert!(pointer.atomic_load() == pointer);
    }
}
