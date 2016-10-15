use std::mem;
use std::collections::HashSet;
use object_pointer::ObjectPointer;

pub const RESET_LIMIT: usize = 8;

pub struct FinalizerSet {
    pub from: HashSet<ObjectPointer>,
    pub to: HashSet<ObjectPointer>,
    resets: usize,
}

impl FinalizerSet {
    pub fn new() -> Self {
        FinalizerSet {
            from: HashSet::new(),
            to: HashSet::new(),
            resets: 0,
        }
    }

    pub fn insert(&mut self, pointer: ObjectPointer) {
        self.from.insert(pointer);
    }

    pub fn remove(&mut self, pointer: &ObjectPointer) {
        self.from.remove(pointer);
        self.to.remove(pointer);
    }

    pub fn retain(&mut self, pointer_ref: &ObjectPointer) {
        if let Some(pointer) = self.from.take(pointer_ref) {
            self.to.insert(pointer);
        }
    }

    pub fn swap_sets(&mut self) {
        self.from.clear();

        if self.resets >= RESET_LIMIT {
            self.from.shrink_to_fit();
            self.to.shrink_to_fit();

            self.resets = 0;
        } else {
            self.resets += 1;
        }

        mem::swap(&mut self.from, &mut self.to);
    }

    pub fn finalize(&mut self) {
        self.finalize_pointers(&self.from);
        self.swap_sets();
    }

    pub fn reset(&mut self) {
        self.finalize_pointers(&self.from);
        self.finalize_pointers(&self.to);

        self.from.clear();
        self.to.clear();

        self.from.shrink_to_fit();
        self.to.shrink_to_fit();
    }

    fn finalize_pointers(&self, pointers: &HashSet<ObjectPointer>) {
        for pointer in pointers.iter() {
            let mut object = pointer.get_mut();

            object.deallocate_pointers();

            drop(object);
        }
    }
}
