//! Types for indexing various tables such as method tables.

/// An index used for accessing classes.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub(crate) struct ClassIndex(u32);

impl ClassIndex {
    pub(crate) fn new(index: u32) -> Self {
        Self(index)
    }
}

impl From<ClassIndex> for u32 {
    fn from(index: ClassIndex) -> u32 {
        index.0
    }
}

impl From<ClassIndex> for usize {
    fn from(index: ClassIndex) -> usize {
        index.0 as usize
    }
}

/// An index used for accessing methods.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub(crate) struct MethodIndex(u16);

impl MethodIndex {
    pub(crate) fn new(index: u16) -> Self {
        Self(index)
    }
}

impl From<MethodIndex> for u16 {
    fn from(index: MethodIndex) -> u16 {
        index.0
    }
}

impl From<MethodIndex> for usize {
    fn from(index: MethodIndex) -> usize {
        index.0 as usize
    }
}

/// An index used for accessing fields.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub(crate) struct FieldIndex(u8);

impl FieldIndex {
    pub(crate) fn new(index: u8) -> Self {
        Self(index)
    }
}

impl From<FieldIndex> for u8 {
    fn from(index: FieldIndex) -> u8 {
        index.0
    }
}

impl From<FieldIndex> for usize {
    fn from(index: FieldIndex) -> usize {
        index.0 as usize
    }
}
