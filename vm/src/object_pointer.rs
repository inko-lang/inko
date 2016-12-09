use std::mem::transmute;
use std::hash::{Hash, Hasher};

use immix::bitmap::{Bitmap, ObjectMap};
use immix::block;
use immix::bucket::{MATURE, MAILBOX, PERMANENT};
use immix::local_allocator::YOUNG_MAX_AGE;

use object::{Object, ObjectStatus};
use process::RcProcess;
use tagged_pointer::TaggedPointer;

/// Performs a write to an object and tracks it in the write barrier.
macro_rules! write_object {
    ($receiver: expr, $process: expr, $action: expr, $value: expr) => ({
        let track = !$receiver.get().has_header();
        let pointer = *$receiver;

        $action;

        $process.write_barrier(pointer, $value);

        if track && $receiver.is_finalizable() {
            $receiver.mark_for_finalization();
        }
    })
}

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

/// The mask to use for tagging a pointer as an integer.
pub const INTEGER_MARK: usize = 0x1; // TODO: implement integers

/// The mask to use for forwarding pointers
pub const FORWARDING_MASK: usize = 0x2;

impl ObjectPointer {
    pub fn new(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer { raw: TaggedPointer::new(pointer) }
    }

    /// Creates a new null pointer.
    pub fn null() -> ObjectPointer {
        ObjectPointer { raw: TaggedPointer::null() }
    }

    /// Returns a forwarding pointer to the current pointer.
    pub fn forwarding_pointer(&self) -> ObjectPointer {
        let raw = TaggedPointer::with_mask(self.raw.raw, FORWARDING_MASK);

        ObjectPointer { raw: raw }
    }

    /// Returns true if the current pointer points to a forwarded object.
    #[inline(always)]
    pub fn is_forwarded(&self) -> bool {
        let object = self.get();

        if object.prototype.is_null() {
            false
        } else {
            object.prototype.raw.mask_is_set(FORWARDING_MASK)
        }
    }

    /// Returns the status of the object.
    #[inline(always)]
    pub fn status(&self) -> ObjectStatus {
        if self.is_forwarded() {
            return ObjectStatus::Resolve;
        }

        let block = self.block();

        if block.bucket().unwrap().promote {
            return ObjectStatus::Promote;
        }

        if block.fragmented {
            return ObjectStatus::Evacuate;
        }

        ObjectStatus::OK
    }

    /// Replaces the current pointer with a pointer to the forwarded object.
    pub fn resolve_forwarding_pointer(&self) {
        let object = self.get();

        if let Some(proto) = object.prototype() {
            // Since object pointers are _usually_ immutable we have to use an
            // extra layer of indirection to update "self".
            unsafe {
                let self_ptr = self as *const ObjectPointer as *mut ObjectPointer;
                let mut self_ref = &mut *self_ptr;

                self_ref.raw = proto.raw.without_tags();
            };
        }
    }

    /// Returns an immutable reference to the Object.
    #[inline(always)]
    pub fn get(&self) -> &Object {
        self.raw.as_ref().unwrap()
    }

    /// Returns a mutable reference to the Object.
    #[inline(always)]
    pub fn get_mut(&self) -> &mut Object {
        self.raw.as_mut().unwrap()
    }

    /// Returns true if the current pointer is a null pointer.
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.raw.raw as usize == 0x0
    }

    /// Returns true if the current pointer points to a permanent object.
    pub fn is_permanent(&self) -> bool {
        self.block().bucket().unwrap().age == PERMANENT
    }

    /// Returns true if the current pointer points to a mature object.
    pub fn is_mature(&self) -> bool {
        self.block().bucket().unwrap().age == MATURE
    }

    /// Returns true if the current pointer points to a mailbox object.
    pub fn is_mailbox(&self) -> bool {
        self.block().bucket().unwrap().age == MAILBOX
    }

    /// Returns true if the current pointer points to a young object.
    pub fn is_young(&self) -> bool {
        self.block().bucket().unwrap().age <= YOUNG_MAX_AGE
    }

    /// Returns true if the current object is marked.
    pub fn is_marked(&self) -> bool {
        let bitmap = self.marked_objects_bitmap();
        let index = self.marked_objects_bitmap_index();

        bitmap.is_set(index)
    }

    /// Marks the line this object resides in.
    pub fn mark_line(&self) {
        let line_index = self.line_index();

        self.block_mut().used_lines_bitmap.set(line_index);
    }

    pub fn mark_for_finalization(&self) {
        let index = self.marked_objects_bitmap_index();

        self.block_mut().finalize_bitmap.set(index);
    }

    pub fn unmark_for_finalization(&self) {
        let index = self.marked_objects_bitmap_index();

        self.block_mut().finalize_bitmap.unset(index);
    }

    /// Marks the current object and its line.
    pub fn mark(&self) {
        let index = self.marked_objects_bitmap_index();

        self.marked_objects_bitmap().set(index);
        self.mark_line();
    }

    /// Returns the mark bitmap to use for this pointer.
    pub fn marked_objects_bitmap(&self) -> &mut ObjectMap {
        &mut self.block_mut().marked_objects_bitmap
    }

    /// Returns the mark bitmap index to use for this pointer.
    pub fn marked_objects_bitmap_index(&self) -> usize {
        let start_addr = self.block_header_pointer_address();
        let offset = self.raw.untagged() as usize - start_addr;

        offset / block::BYTES_PER_OBJECT
    }

    /// Returns the line index of the current pointer.
    #[inline(always)]
    pub fn line_index(&self) -> usize {
        self.block().line_index_of_pointer(self.raw.untagged())
    }

    /// Returns a mutable reference to the block this pointer belongs to.
    #[inline(always)]
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
        unsafe {
            let ptr: *mut block::BlockHeader =
                transmute(self.block_header_pointer_address());

            &*ptr
        }
    }

    /// Returns true if the object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        self.get().is_finalizable()
    }

    /// Adds a method to the object this pointer points to.
    pub fn add_method(&self,
                      process: &RcProcess,
                      name: String,
                      method: ObjectPointer) {
        write_object!(self,
                      process,
                      self.get_mut().add_method(name, method),
                      method);
    }

    /// Adds an attribute to the object this pointer points to.
    pub fn add_attribute(&self,
                         process: &RcProcess,
                         name: String,
                         attr: ObjectPointer) {
        write_object!(self,
                      process,
                      self.get_mut().add_attribute(name, attr),
                      attr);
    }

    /// Adds a constant to the object this pointer points to.
    pub fn add_constant(&self,
                        process: &RcProcess,
                        name: String,
                        constant: ObjectPointer) {
        write_object!(self,
                      process,
                      self.get_mut().add_constant(name, constant),
                      constant);
    }

    /// Sets the outer scope of the object this pointer points to.
    pub fn set_outer_scope(&self, process: &RcProcess, scope: ObjectPointer) {
        write_object!(self,
                      process,
                      self.get_mut().set_outer_scope(scope),
                      scope);
    }

    /// Returns a pointer to this pointer.
    pub fn pointer(&self) -> ObjectPointerPointer {
        ObjectPointerPointer::new(self)
    }

    fn block_header_pointer_address(&self) -> usize {
        (self.raw.untagged() as isize & block::OBJECT_BITMAP_MASK) as usize
    }
}

impl ObjectPointerPointer {
    pub fn new(pointer: &ObjectPointer) -> ObjectPointerPointer {
        ObjectPointerPointer { raw: pointer as *const ObjectPointer }
    }

    #[inline(always)]
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use super::*;
    use immix::bitmap::Bitmap;
    use immix::global_allocator::GlobalAllocator;
    use immix::local_allocator::LocalAllocator;
    use immix::bucket::{Bucket, MATURE, MAILBOX, PERMANENT};
    use immix::block::Block;
    use object::{Object, ObjectStatus};
    use object_value::ObjectValue;

    fn fake_raw_pointer() -> RawObjectPointer {
        0x4 as RawObjectPointer
    }

    fn object_pointer_for(object: &Object) -> ObjectPointer {
        ObjectPointer::new(object as *const Object as RawObjectPointer)
    }

    fn local_allocator() -> LocalAllocator {
        LocalAllocator::new(GlobalAllocator::without_preallocated_blocks())
    }

    fn buckets_for_all_ages() -> (Bucket, Bucket, Bucket, Bucket) {
        let young = Bucket::with_age(0);
        let mature = Bucket::with_age(MATURE);
        let mailbox = Bucket::with_age(MAILBOX);
        let permanent = Bucket::with_age(PERMANENT);

        (young, mature, mailbox, permanent)
    }

    fn allocate_in_bucket(bucket: &mut Bucket) -> ObjectPointer {
        if bucket.blocks.len() == 0 {
            bucket.add_block(Block::new());
        }

        bucket.current_block_mut()
            .unwrap()
            .bump_allocate(Object::new(ObjectValue::None))
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
    fn test_object_pointer_forwarding_pointer() {
        let pointer = ObjectPointer::null().forwarding_pointer();

        assert!(pointer.raw.mask_is_set(FORWARDING_MASK));
    }

    #[test]
    fn test_object_pointer_is_forwarded_with_regular_pointer() {
        let object = Object::new(ObjectValue::None);
        let pointer = object_pointer_for(&object);

        assert_eq!(pointer.is_forwarded(), false);
    }

    #[test]
    fn test_object_pointer_is_forwarded_with_forwarding_pointer() {
        let object = Object::new(ObjectValue::None);
        let pointer = object_pointer_for(&object).forwarding_pointer();

        assert_eq!(pointer.is_forwarded(), false);
    }

    #[test]
    fn test_object_pointer_status() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();
        let pointer2 = allocator.allocate_empty();

        assert!(match pointer.status() {
            ObjectStatus::OK => true,
            _ => false,
        });

        pointer.block_mut().bucket_mut().unwrap().promote = true;

        assert!(match pointer.status() {
            ObjectStatus::Promote => true,
            _ => false,
        });

        pointer.block_mut().bucket_mut().unwrap().promote = false;
        pointer.block_mut().set_fragmented();

        assert!(match pointer.status() {
            ObjectStatus::Evacuate => true,
            _ => false,
        });

        pointer.get_mut().forward_to(pointer2);

        assert!(match pointer.status() {
            ObjectStatus::Resolve => true,
            _ => false,
        });
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer() {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Integer(2));

        object.set_prototype(proto_pointer.forwarding_pointer());

        let pointer = object_pointer_for(&object);

        pointer.resolve_forwarding_pointer();

        assert!(pointer == proto_pointer);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer_in_vector() {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Integer(2));

        object.set_prototype(proto_pointer.forwarding_pointer());

        let pointers = vec![object_pointer_for(&object)];

        pointers.get(0).unwrap().resolve_forwarding_pointer();

        assert!(pointers[0] == proto_pointer);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer_in_vector_with_pointer_pointers
        () {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Integer(2));

        object.set_prototype(proto_pointer.forwarding_pointer());

        let mut pointers = vec![object_pointer_for(&object)];
        let mut pointer_pointers = vec![&mut pointers[0] as *mut ObjectPointer];

        let ptr_ref = unsafe { &mut *pointer_pointers[0] };

        ptr_ref.resolve_forwarding_pointer();

        assert!(pointers[0] == proto_pointer);
    }

    #[test]
    fn test_object_pointer_get_get_mut() {
        let object = Object::new(ObjectValue::Integer(2));
        let pointer = object_pointer_for(&object);

        // Object doesn't implement PartialEq/Eq so we can't compare references,
        // thus we'll just test if we get something somewhat correct-ish.
        assert!(pointer.get().value.is_integer());
        assert!(pointer.get_mut().value.is_integer());
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
    fn test_object_pointer_mark_line() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.mark_line();

        assert!(pointer.block().used_lines_bitmap.is_set(1));
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
    fn test_object_pointer_marked_objects_bitmap() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.marked_objects_bitmap();
    }

    #[test]
    fn test_object_pointer_marked_objects_bitmap_index() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        assert_eq!(pointer.marked_objects_bitmap_index(), 4);
    }

    #[test]
    fn test_object_pointer_line_index() {
        let mut allocator = local_allocator();

        let ptr1 = allocator.allocate_empty();
        let ptr2 = allocator.allocate_empty();
        let ptr3 = allocator.allocate_empty();
        let ptr4 = allocator.allocate_empty();
        let ptr5 = allocator.allocate_empty();

        assert_eq!(ptr1.line_index(), 1);
        assert_eq!(ptr2.line_index(), 1);
        assert_eq!(ptr3.line_index(), 1);
        assert_eq!(ptr4.line_index(), 1);
        assert_eq!(ptr5.line_index(), 2);
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
        let mut reference = raw_pointer.get_mut();

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
    fn test_object_pointer_pointer_get_mut() {
        let ptr = ObjectPointer::new(fake_raw_pointer());
        let ptr_ptr = ptr.pointer();

        ptr_ptr.get_mut().raw.raw = 0x5 as RawObjectPointer;

        assert_eq!(ptr.raw.raw as usize, 0x5);
    }
}
