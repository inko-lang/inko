use tir::expression::Expression;

#[derive(Debug)]
pub struct Rename {
    pub original: String,
    pub alias: String,
}

#[derive(Debug)]
pub struct Implement {
    pub constant: Expression,
    pub renames: Vec<Rename>,
    pub line: usize,
    pub column: usize,
}

impl Rename {
    pub fn new(original: String, alias: String) -> Self {
        Rename { original: original, alias: alias }
    }
}

impl Implement {
    pub fn new(
        constant: Expression,
        renames: Vec<Rename>,
        line: usize,
        column: usize,
    ) -> Self {
        Implement {
            constant: constant,
            renames: renames,
            line: line,
            column: column,
        }
    }
}
