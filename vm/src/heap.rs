//! Storing of runtime objects on the heap
//!
//! A Heap can be used to store objects that are created during the lifetime of
//! a program. These objects are garbage collected whenever they are no longer
//! in use.

use object::Object;
use object_value;
use object_value::ObjectValue;
use object_pointer::{ObjectPointer, RawObjectPointer};

const PAGE_SLOTS: usize = 128;

pub struct HeapPage {
    slots: Vec<Option<Object>>,
}

pub struct Heap {
    pub pages: Vec<HeapPage>,
    pub global: bool,
}

impl HeapPage {
    pub fn new() -> HeapPage {
        HeapPage { slots: Vec::with_capacity(PAGE_SLOTS) }
    }

    /// Returns true if the current page has space at the end for more objects.
    pub fn has_space(&self) -> bool {
        if self.slots.len() < self.slots.capacity() {
            true
        } else {
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
    pub fn new(global: bool) -> Heap {
        let mut heap = Heap {
            pages: Vec::with_capacity(1),
            global: global,
        };

        heap.add_page();

        heap
    }

    pub fn local() -> Heap {
        Heap::new(false)
    }

    pub fn global() -> Heap {
        Heap::new(true)
    }

    /// Allocates the object on a page.
    ///
    /// This method always allocates the object in the last available page. If
    /// no page is available a new one is allocated.
    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
        self.ensure_page_exists();
        self.ensure_last_page_has_space();

        let mut last_page = self.pages.last_mut().unwrap();
        let raw_pointer = last_page.allocate(object);
        let pointer = ObjectPointer::new(raw_pointer);

        pointer.get_mut().set_permanent();

        pointer
    }

    pub fn allocate_value(&mut self, value: ObjectValue) -> ObjectPointer {
        let obj = Object::new(value);

        self.allocate(obj)
    }

    pub fn allocate_value_with_prototype(&mut self,
                                         value: ObjectValue,
                                         proto: ObjectPointer)
                                         -> ObjectPointer {
        let obj = Object::with_prototype(value, proto);

        self.allocate(obj)
    }

    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.allocate_value(object_value::none())
    }

    pub fn add_page(&mut self) {
        self.pages.push(HeapPage::new());
    }

    /// Performs a deep copy of `object_ptr`
    ///
    /// The copy of the input object is allocated on the current process' heap.
    /// Values such as Arrays are recursively copied.
    pub fn copy_object(&mut self, to_copy_ptr: ObjectPointer) -> ObjectPointer {
        if to_copy_ptr.is_permanent() {
            return to_copy_ptr;
        }

        let to_copy = to_copy_ptr.get();

        // Copy over the object value
        let value_copy = match to_copy.value {
            ObjectValue::None => object_value::none(),
            ObjectValue::Integer(num) => object_value::integer(num),
            ObjectValue::Float(num) => object_value::float(num),
            ObjectValue::String(ref string) => {
                object_value::string(*string.clone())
            }
            ObjectValue::Array(ref raw_vec) => {
                let new_map = raw_vec.iter()
                    .map(|val_ptr| self.copy_object(*val_ptr));

                object_value::array(new_map.collect::<Vec<_>>())
            }
            ObjectValue::File(_) => {
                panic!("ObjectValue::File can not be cloned");
            }
            ObjectValue::Error(num) => object_value::error(num),
            ObjectValue::CompiledCode(ref code) => {
                object_value::compiled_code(code.clone())
            }
            ObjectValue::Binding(_) => {
                panic!("ObjectValue::Binding can not be cloned");
            }
        };

        let mut copy = if let Some(proto_ptr) = to_copy.prototype() {
            Object::with_prototype(value_copy, self.copy_object(proto_ptr))
        } else {
            Object::new(value_copy)
        };

        if let Some(header) = to_copy.header() {
            copy.set_header(header.copy_to(self));
        }

        self.allocate(copy)
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
        } else {
            add_page = true;
        }

        if add_page {
            self.add_page();
        }
    }
}
