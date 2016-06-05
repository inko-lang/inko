//! Storing of runtime objects on the heap
//!
//! A Heap can be used to store objects that are created during the lifetime of
//! a program. These objects are garbage collected whenever they are no longer
//! in use.

use object::Object;
use object_value;
use object_pointer::{RawObjectPointer, ObjectPointer};

const PAGE_SLOTS: usize = 128;
const PAGE_COUNT: usize = 1;

pub struct HeapPage {
    slots: Vec<Option<Object>>
}

pub struct Heap {
    pub pages: Vec<HeapPage>
}

impl HeapPage {
    pub fn new() -> HeapPage {
        HeapPage { slots: Vec::with_capacity(PAGE_SLOTS) }
    }

    /// Returns true if the current page has space at the end for more objects.
    pub fn has_space(&self) -> bool {
        if self.slots.len() < self.slots.capacity() {
            true
        }
        else {
            self.slots.last().is_none()
        }
    }

    pub fn allocate(&mut self, object: Object) -> RawObjectPointer {
        self.slots.push(Some(object));

        let index = self.slots.len() - 1;

        self.slots[index].as_mut().unwrap() as RawObjectPointer
    }
}

impl Heap {
    pub fn new() -> Heap {
        Heap::with_pages(PAGE_COUNT)
    }

    /// Allocates a heap with `count` pre-allocated pages.
    pub fn with_pages(count: usize) -> Heap {
        let mut heap = Heap { pages: Vec::with_capacity(count) };

        for _ in 0..PAGE_COUNT {
            heap.add_page();
        }

        heap
    }

    /// Allocates the object on a page.
    ///
    /// This method always allocates the object in the last available page. If
    /// no page is available a new one is allocated.
    pub fn allocate(&mut self, object: Object) -> RawObjectPointer {
        self.ensure_page_exists();
        self.ensure_last_page_has_space();

        let mut last_page = self.pages.last_mut().unwrap();

        last_page.allocate(object)
    }

    pub fn allocate_global(&mut self, object: Object) -> ObjectPointer {
        let ptr = self.allocate(object);

        ObjectPointer::global(ptr)
    }

    pub fn allocate_local(&mut self, object: Object) -> ObjectPointer {
        let ptr = self.allocate(object);

        ObjectPointer::local(ptr)
    }

    pub fn allocate_empty_global(&mut self) -> ObjectPointer {
        let obj = Object::new(object_value::none());

        self.allocate_global(obj)
    }

    pub fn add_page(&mut self) {
        self.pages.push(HeapPage::new());
    }

    fn ensure_page_exists(&mut self) {
        if self.pages.len() == 0 {
            self.add_page();
        }
    }

    /// Ensure the last page always has a slot available for the object.
    fn ensure_last_page_has_space(&mut self) {
        let mut add_page = false;

        if let Some(last_page) = self.pages.last() {
            if !last_page.has_space() {
                add_page = true;
            }
        }
        else {
            add_page = true;
        }

        if add_page {
            self.add_page();
        }
    }
}
