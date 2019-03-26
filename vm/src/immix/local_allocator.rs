//! Process-local memory allocator
//!
//! The LocalAllocator lives in a Process and is used for allocating memory on a
//! process heap.
use std::collections::HashSet;

use config::Config;
use gc::work_list::WorkList;
use immix::bucket::{Bucket, MATURE};
use immix::copy_object::CopyObject;
use immix::generation_config::GenerationConfig;
use immix::global_allocator::RcGlobalAllocator;
use immix::histograms::Histograms;
use object::Object;
use object_pointer::ObjectPointer;
use object_value;
use object_value::ObjectValue;
use vm::state::RcState;

/// The maximum age of a bucket in the young generation.
pub const YOUNG_MAX_AGE: i8 = 2;

/// Structure containing the state of a process-local allocator.
pub struct LocalAllocator {
    /// The global allocated from which to request blocks of memory and return
    /// unused blocks to.
    pub global_allocator: RcGlobalAllocator,

    /// The buckets to use for the eden and young survivor spaces.
    pub young_generation: [Bucket; YOUNG_MAX_AGE as usize + 1],

    /// The histograms to use for collecting the young generation.
    pub young_histograms: Histograms,

    /// The histograms to use for collecting the mature generation.
    pub mature_histograms: Histograms,

    /// The position of the eden bucket in the young generation.
    pub eden_index: usize,

    /// The remembered set of this process. This set is not synchronized via a
    /// lock of sorts. As such the collector must ensure this process is
    /// suspended upon examining the remembered set.
    pub remembered_set: HashSet<ObjectPointer>,

    /// The bucket to use for the mature generation.
    pub mature_generation: Bucket,

    /// The configuration for the young generation.
    pub young_config: GenerationConfig,

    /// The configuration for the mature generation.
    pub mature_config: GenerationConfig,
}

impl LocalAllocator {
    pub fn new(
        global_allocator: RcGlobalAllocator,
        config: &Config,
    ) -> LocalAllocator {
        LocalAllocator {
            global_allocator,
            young_generation: [
                Bucket::with_age(0),
                Bucket::with_age(-1),
                Bucket::with_age(-2),
            ],
            young_histograms: Histograms::new(),
            mature_histograms: Histograms::new(),
            eden_index: 0,
            mature_generation: Bucket::with_age(MATURE),
            young_config: GenerationConfig::new(config.young_threshold),
            mature_config: GenerationConfig::new(config.mature_threshold),
            remembered_set: HashSet::new(),
        }
    }

    pub fn global_allocator(&self) -> RcGlobalAllocator {
        self.global_allocator.clone()
    }

    pub fn eden_space(&self) -> &Bucket {
        &self.young_generation[self.eden_index]
    }

    pub fn eden_space_mut(&mut self) -> &mut Bucket {
        &mut self.young_generation[self.eden_index]
    }

    pub fn should_collect_young(&self) -> bool {
        self.young_config.allocation_threshold_exceeded()
    }

    pub fn should_collect_mature(&self) -> bool {
        self.mature_config.allocation_threshold_exceeded()
    }

    /// Prepares for a garbage collection.
    ///
    /// Returns true if objects have to be moved around.
    pub fn prepare_for_collection(&mut self, mature: bool) -> bool {
        let mut move_objects = false;

        for bucket in &mut self.young_generation {
            if bucket.prepare_for_collection(&self.young_histograms) {
                move_objects = true;
            }

            if bucket.promote {
                move_objects = true;
            }
        }

        if mature {
            if self
                .mature_generation
                .prepare_for_collection(&self.mature_histograms)
            {
                move_objects = true;
            }
        } else if self.has_remembered_objects() {
            self.prepare_remembered_objects_for_collection();
        }

        move_objects
    }

    /// Reclaims blocks in the young (and mature) generation.
    pub fn reclaim_blocks(&mut self, state: &RcState, mature: bool) {
        self.young_histograms.reset();

        for bucket in &mut self.young_generation {
            bucket.reclaim_blocks(state, &self.young_histograms);
        }

        if mature {
            self.mature_histograms.reset();

            self.mature_generation
                .reclaim_blocks(state, &self.mature_histograms);
        } else {
            for block in self.mature_generation.blocks.iter_mut() {
                block.update_line_map();
            }
        }
    }

    pub fn allocate_with_prototype(
        &mut self,
        value: ObjectValue,
        proto: ObjectPointer,
    ) -> ObjectPointer {
        let object = Object::with_prototype(value, proto);

        self.allocate_eden(object)
    }

    pub fn allocate_without_prototype(
        &mut self,
        value: ObjectValue,
    ) -> ObjectPointer {
        let object = Object::new(value);

        self.allocate_eden(object)
    }

    /// Allocates an empty object without a prototype.
    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.allocate_without_prototype(object_value::none())
    }

    pub fn allocate_eden(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = self.allocate_eden_raw(object);

        if new_block {
            self.young_config.increment_allocations();
        }

        pointer
    }

    pub fn allocate_mature(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = self.allocate_mature_raw(object);

        if new_block {
            self.mature_config.increment_allocations();
        }

        pointer
    }

    /// Increments the age of all buckets in the young generation
    pub fn increment_young_ages(&mut self) {
        for (index, bucket) in self.young_generation.iter_mut().enumerate() {
            if bucket.age == YOUNG_MAX_AGE {
                bucket.reset_age();
            } else {
                bucket.increment_age();
                bucket.promote = bucket.age == YOUNG_MAX_AGE;
            }

            if bucket.age == 0 {
                self.eden_index = index;
            }
        }
    }

    pub fn update_collection_statistics(
        &mut self,
        config: &Config,
        mature: bool,
    ) {
        self.update_young_collection_statistics(config);

        if mature {
            self.update_mature_collection_statistics(config);
        }
    }

    pub fn update_young_collection_statistics(&mut self, config: &Config) {
        self.increment_young_ages();

        self.young_config.block_allocations = 0;

        let blocks = self.number_of_young_blocks();
        let max = config.heap_growth_threshold;
        let factor = config.heap_growth_factor;

        if self.young_config.should_increase_threshold(blocks, max) {
            self.young_config.increment_threshold(factor);
        }
    }

    pub fn update_mature_collection_statistics(&mut self, config: &Config) {
        self.update_young_collection_statistics(config);

        self.mature_config.block_allocations = 0;

        let blocks = self.mature_generation.number_of_blocks();
        let max = config.heap_growth_threshold;
        let factor = config.heap_growth_factor;

        if self.mature_config.should_increase_threshold(blocks, max) {
            self.mature_config.increment_threshold(factor);
        }
    }

    pub fn number_of_young_blocks(&self) -> u32 {
        self.young_generation
            .iter()
            .map(|bucket| bucket.number_of_blocks())
            .sum()
    }

    pub fn has_remembered_objects(&self) -> bool {
        !self.remembered_set.is_empty()
    }

    pub fn remember_object(&mut self, pointer: ObjectPointer) {
        self.remembered_set.insert(pointer);
    }

    pub fn prune_remembered_objects(&mut self) {
        self.remembered_set.retain(|p| p.is_marked());
    }

    pub fn prepare_remembered_objects_for_collection(&mut self) {
        for pointer in &self.remembered_set {
            // We prepare the entire block because this is simpler than having
            // to figure out if we can unmark a line or not (since a line may
            // contain multiple objects).
            pointer.block_mut().prepare_for_collection();
        }
    }

    pub fn remembered_pointers(&self) -> WorkList {
        let mut pointers = WorkList::new();

        for pointer in &self.remembered_set {
            pointers.push(pointer.pointer());
        }

        pointers
    }

    fn allocate_eden_raw(&mut self, object: Object) -> (bool, ObjectPointer) {
        unsafe {
            self.young_generation[self.eden_index]
                .allocate_for_mutator(&self.global_allocator, object)
        }
    }

    fn allocate_mature_raw(&mut self, object: Object) -> (bool, ObjectPointer) {
        self.mature_generation
            .allocate(&self.global_allocator, object)
    }
}

impl CopyObject for LocalAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate_eden(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;
    use immix::copy_object::CopyObject;
    use immix::global_allocator::GlobalAllocator;
    use object::Object;
    use object_value;
    use std::mem;
    use vm::state::{RcState, State};

    fn local_allocator() -> (RcState, LocalAllocator) {
        let state = State::with_rc(Config::new(), &[]);
        let alloc =
            LocalAllocator::new(GlobalAllocator::with_rc(), &state.config);

        (state, alloc)
    }

    #[test]
    fn test_new() {
        let (_, alloc) = local_allocator();

        assert_eq!(alloc.young_generation[0].age, 0);
        assert_eq!(alloc.young_generation[1].age, -1);
        assert_eq!(alloc.young_generation[2].age, -2);
        assert_eq!(alloc.eden_index, 0);
    }

    #[test]
    fn test_eden_space_mut() {
        let (_, mut alloc) = local_allocator();

        assert_eq!(alloc.eden_space_mut().age, 0);
    }

    #[test]
    fn test_prepare_for_collection() {
        let (_, mut alloc) = local_allocator();

        assert_eq!(alloc.prepare_for_collection(true), false);

        alloc.young_generation[0].promote = true;

        assert_eq!(alloc.prepare_for_collection(true), true);
    }

    #[test]
    fn test_reclaim_blocks() {
        let (state, mut alloc) = local_allocator();

        let block1 = alloc.global_allocator.request_block();
        let block2 = alloc.global_allocator.request_block();

        alloc.eden_space_mut().add_block(block1);
        alloc.mature_generation.add_block(block2);

        alloc.reclaim_blocks(&state, false);

        assert_eq!(alloc.eden_space_mut().blocks.len(), 0);
        assert_eq!(alloc.mature_generation.blocks.len(), 1);

        alloc.reclaim_blocks(&state, true);

        assert_eq!(alloc.mature_generation.blocks.len(), 0);
    }

    #[test]
    fn test_allocate_with_prototype() {
        let (_, mut alloc) = local_allocator();
        let proto = alloc.allocate_empty();
        let pointer =
            alloc.allocate_with_prototype(object_value::float(5.0), proto);

        assert!(pointer.get().prototype == proto);
        assert!(pointer.get().value.is_float());
    }

    #[test]
    fn test_allocate_without_prototype() {
        let (_, mut alloc) = local_allocator();
        let pointer =
            alloc.allocate_without_prototype(object_value::float(5.0));

        assert!(pointer.get().prototype().is_none());
        assert!(pointer.get().value.is_float());
    }

    #[test]
    fn test_allocate_empty() {
        let (_, mut alloc) = local_allocator();
        let pointer = alloc.allocate_empty();

        assert!(pointer.get().value.is_none());
        assert!(pointer.get().prototype().is_none());
    }

    #[test]
    fn test_allocate_eden() {
        let (_, mut alloc) = local_allocator();
        let ptr1 = alloc.allocate_eden(Object::new(object_value::none()));

        let ptr2 = alloc
            .allocate_eden(Object::new(object_value::string("a".to_string())));

        assert!(ptr1.is_young());
        assert!(ptr2.is_young());
    }

    #[test]
    fn test_allocate_mature() {
        let (_, mut alloc) = local_allocator();
        let ptr1 = alloc.allocate_mature(Object::new(object_value::none()));

        let ptr2 = alloc.allocate_mature(Object::new(object_value::string(
            "a".to_string(),
        )));

        assert!(ptr1.is_mature());
        assert!(ptr2.is_mature());
    }

    #[test]
    fn test_increment_young_ages() {
        let (_, mut alloc) = local_allocator();

        assert_eq!(alloc.young_generation[0].age, 0);
        assert_eq!(alloc.young_generation[1].age, -1);
        assert_eq!(alloc.young_generation[2].age, -2);
        assert_eq!(alloc.eden_index, 0);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 1);
        assert_eq!(alloc.young_generation[1].age, 0);
        assert_eq!(alloc.young_generation[2].age, -1);
        assert_eq!(alloc.eden_index, 1);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 2);
        assert_eq!(alloc.young_generation[1].age, 1);
        assert_eq!(alloc.young_generation[2].age, 0);
        assert_eq!(alloc.young_generation[0].promote, true);
        assert_eq!(alloc.eden_index, 2);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 0);
        assert_eq!(alloc.young_generation[1].age, 2);
        assert_eq!(alloc.young_generation[2].age, 1);
        assert_eq!(alloc.young_generation[1].promote, true);
        assert_eq!(alloc.eden_index, 0);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 1);
        assert_eq!(alloc.young_generation[0].promote, false);

        assert_eq!(alloc.young_generation[1].age, 0);
        assert_eq!(alloc.young_generation[1].promote, false);

        assert_eq!(alloc.young_generation[2].age, 2);
        assert_eq!(alloc.young_generation[2].promote, true);
        assert_eq!(alloc.eden_index, 1);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 2);
        assert_eq!(alloc.young_generation[1].age, 1);
        assert_eq!(alloc.young_generation[2].age, 0);
        assert_eq!(alloc.eden_index, 2);
    }

    #[test]
    fn test_copy_object() {
        let (_, mut alloc) = local_allocator();
        let pointer =
            alloc.allocate_without_prototype(object_value::float(5.0));

        let copy = alloc.copy_object(pointer);

        assert!(copy.is_young());
        assert!(copy.get().value.is_float());
    }

    #[test]
    fn test_remember_object() {
        let (_, mut alloc) = local_allocator();
        let pointer = alloc.allocate_empty();

        alloc.remember_object(pointer);

        assert!(alloc.has_remembered_objects());
    }

    #[test]
    fn test_prune_remembered_objects() {
        let (_, mut alloc) = local_allocator();
        let ptr1 = alloc.allocate_empty();
        let ptr2 = alloc.allocate_empty();

        ptr1.mark();

        alloc.remember_object(ptr1);
        alloc.remember_object(ptr2);
        alloc.prune_remembered_objects();

        assert_eq!(alloc.remembered_set.contains(&ptr1), true);
        assert_eq!(alloc.remembered_set.contains(&ptr2), false);
    }

    #[test]
    fn test_prepare_remembered_objects_for_collection() {
        let (_, mut alloc) = local_allocator();
        let ptr1 = alloc.allocate_empty();

        ptr1.mark();

        alloc.remember_object(ptr1);
        alloc.prepare_remembered_objects_for_collection();

        assert_eq!(ptr1.is_marked(), false);
    }

    #[test]
    fn test_type_size() {
        // This test is put in place to ensure that the type size doesn't change
        // unexpectedly.
        assert_eq!(mem::size_of::<LocalAllocator>(), 264);
    }
}
