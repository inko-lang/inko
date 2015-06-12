//! Caching of constant lookups
//!
//! A ConstantCache can be used to cache any found constants at some point in a
//! program. By caching the constant repeated lookups for the same constant
//! don't have to potentially walk through big object prototype trees.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use object::RcObject;

/// Cache for constants looked up in a given scope.
pub struct ConstantCache {
    pub constants: HashMap<String, RcObject>
}

/// A mutable, reference counted constant cache.
pub type RcConstantCache = Arc<RwLock<ConstantCache>>;

impl ConstantCache {
    /// Creates a new ConstantCache.
    pub fn new() -> ConstantCache {
        ConstantCache { constants: HashMap::new() }
    }

    /// Creates a new reference counted ConstantCache.
    pub fn with_rc() -> RcConstantCache {
        Arc::new(RwLock::new(ConstantCache::new()))
    }

    /// Inserts a new constant into the cache.
    pub fn insert(&mut self, name: String, value: RcObject) {
        self.constants.insert(name, value);
    }

    /// Looks up a constant from the cache.
    pub fn get(&mut self, name: &String) -> Option<RcObject> {
        self.constants.get(name).cloned()
    }
}
