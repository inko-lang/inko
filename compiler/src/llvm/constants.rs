/// The mask to use for tagged integers.
pub(crate) const INT_MASK: i64 = 0b001;

/// The number of bits to shift for tagged integers.
pub(crate) const INT_SHIFT: usize = 1;

/// The minimum integer value that can be stored as a tagged signed integer.
pub(crate) const MIN_INT: i64 = i64::MIN >> INT_SHIFT;

/// The maximum integer value that can be stored as a tagged signed integer.
pub(crate) const MAX_INT: i64 = i64::MAX >> INT_SHIFT;

/// The mask to use to check if a value is a tagged integer or reference.
pub(crate) const TAG_MASK: i64 = 0b11;

/// The mask to apply to get rid of the tagging bits.
pub(crate) const UNTAG_MASK: u64 = (!TAG_MASK) as u64;

/// The offset to apply to access a regular field.
///
/// The object header occupies the first field (as an inline struct), so all
/// user-defined fields start at the next field.
pub(crate) const FIELD_OFFSET: usize = 1;

/// The offset to apply to access a process field.
pub(crate) const PROCESS_FIELD_OFFSET: usize = 2;

/// The mask to use for checking if a value is a reference.
pub(crate) const REF_MASK: i64 = 0b10;

/// The field index of the `State` field that contains the `true` singleton.
pub(crate) const TRUE_INDEX: u32 = 0;

/// The field index of the `State` field that contains the `false` singleton.
pub(crate) const FALSE_INDEX: u32 = 1;

/// The field index of the `State` field that contains the `nil` singleton.
pub(crate) const NIL_INDEX: u32 = 2;

pub(crate) const HEADER_CLASS_INDEX: u32 = 0;
pub(crate) const HEADER_KIND_INDEX: u32 = 1;
pub(crate) const HEADER_REFS_INDEX: u32 = 2;

pub(crate) const BOXED_INT_VALUE_INDEX: u32 = 1;
pub(crate) const BOXED_FLOAT_VALUE_INDEX: u32 = 1;

pub(crate) const CLASS_METHODS_COUNT_INDEX: u32 = 2;
pub(crate) const CLASS_METHODS_INDEX: u32 = 3;

pub(crate) const METHOD_HASH_INDEX: u32 = 0;
pub(crate) const METHOD_FUNCTION_INDEX: u32 = 1;

// The values used to represent the kind of a value/reference. These values
// must match the values used by `Kind` in the runtime library.
pub(crate) const OWNED_KIND: u8 = 0;
pub(crate) const REF_KIND: u8 = 1;
pub(crate) const ATOMIC_KIND: u8 = 2;
pub(crate) const PERMANENT_KIND: u8 = 3;
pub(crate) const INT_KIND: u8 = 4;
pub(crate) const FLOAT_KIND: u8 = 5;
pub(crate) const CONTEXT_STATE_INDEX: u32 = 0;
pub(crate) const CONTEXT_PROCESS_INDEX: u32 = 1;
pub(crate) const CONTEXT_ARGS_INDEX: u32 = 2;
pub(crate) const MESSAGE_ARGUMENTS_INDEX: u32 = 2;
pub(crate) const DROPPER_INDEX: u32 = 0;
pub(crate) const CLOSURE_CALL_INDEX: u32 = 1;
pub(crate) const ARRAY_LENGTH_INDEX: u32 = 1;
pub(crate) const ARRAY_CAPA_INDEX: u32 = 2;
pub(crate) const ARRAY_BUF_INDEX: u32 = 3;
