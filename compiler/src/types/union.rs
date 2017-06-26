use types::Type;

#[derive(Debug)]
pub struct Union {
    /// The types that are unioned together.
    pub members: Vec<Type>,
}
