use config::SOURCE_EXT;
use std::path::MAIN_SEPARATOR;

#[derive(Debug)]
pub struct QualifiedName {
    pub parts: Vec<String>,
}

impl QualifiedName {
    pub fn new(parts: Vec<String>) -> Self {
        assert!(parts.len() > 0);

        QualifiedName { parts: parts }
    }

    pub fn module_name(&self) -> &String {
        self.parts.last().unwrap()
    }

    pub fn source_path_with_extension(&self) -> String {
        self.parts.join(&MAIN_SEPARATOR.to_string()) + SOURCE_EXT
    }
}
