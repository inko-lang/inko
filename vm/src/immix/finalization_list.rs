//! Data structure for storing pointers to finalize.
use rayon::prelude::*;
use std::ops::Drop;

use object::AttributesMap;
use object_value::ObjectValue;

/// A value to finalize.
pub enum FinalizeValue {
    ObjectValue(ObjectValue),
    AttributesPointer(*const AttributesMap),
}

unsafe impl Sync for FinalizeValue {}
unsafe impl Send for FinalizeValue {}

impl Drop for FinalizeValue {
    fn drop(&mut self) {
        match self {
            &mut FinalizeValue::AttributesPointer(pointer) => {
                let boxed = unsafe {
                    Box::from_raw(pointer as *mut AttributesMap);
                };

                drop(boxed);
            }
            _ => {}
        }
    }
}

/// A list of pointers to finalize after reclaiming Immix blocks.
pub struct FinalizationList {
    pub values: Vec<FinalizeValue>,
}

impl FinalizationList {
    pub fn new() -> Self {
        FinalizationList { values: Vec::new() }
    }

    pub fn append(&mut self, mut list: FinalizationList) {
        self.values.append(&mut list.values);
    }

    pub fn push_value(&mut self, value: ObjectValue) {
        self.values.push(FinalizeValue::ObjectValue(value));
    }

    pub fn push_attributes(&mut self, value: *const AttributesMap) {
        self.values.push(FinalizeValue::AttributesPointer(value));
    }

    pub fn parallel_finalize(self) {
        self.values.into_par_iter().for_each(|value| drop(value));
    }

    pub fn finalize(self) {
        self.values.into_iter().for_each(|value| drop(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_value;

    #[test]
    fn test_append() {
        let val1 = object_value::integer(0);
        let val2 = object_value::integer(1);
        let mut list1 = FinalizationList::new();
        let mut list2 = FinalizationList::new();

        list1.push_value(val1);
        list2.push_value(val2);
        list1.append(list2);

        assert_eq!(list1.values.len(), 2);
    }

    #[test]
    fn test_push_value() {
        let mut list = FinalizationList::new();
        let val = object_value::integer(0);

        list.push_value(val);

        assert_eq!(list.values.len(), 1);
    }

    #[test]
    fn test_finalize() {
        let string = object_value::string("hello".to_string());
        let mut list = FinalizationList::new();

        list.push_value(string);
        list.finalize();
    }
}
