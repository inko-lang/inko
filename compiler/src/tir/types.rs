#[derive(Debug, Clone)]
pub enum Type {
    /// The type is not yet known. This value should only be used when setting
    /// up TIR in its initial form, prior to performing type inference.
    Unknown,

    /// A dynamic type that may change during runtime.
    Dynamic,

    /// A statically known type
    Static {
        /// The name of the type.
        name: String,

        /// Any type arguments that were defined.
        arguments: Vec<Type>,

        /// Any traits that must be implemented by this type.
        required_traits: Vec<Type>,

        /// The line number on which the type was defined.
        line: usize,

        /// The column on which the type was defined.
        column: usize,
    },
}
