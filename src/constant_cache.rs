use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::RwLock;

use object::RcObjectType;

/// Cache for constants looked up in a given scope.
///
/// This struct can be used to cache constants available in a given scope,
/// removing the need for full constant lookups on every reference.
///
pub struct ConstantCache {
    pub constants: RwLock<HashMap<String, RcObjectType>>
}

/// A mutable, reference counted constant cache.
pub type RcConstantCache = Rc<RefCell<ConstantCache>>;

impl ConstantCache {
    /// Creates a new ConstantCache.
    pub fn new() -> ConstantCache {
        ConstantCache { constants: RwLock::new(HashMap::new()) }
    }

    /// Creates a new reference counted ConstantCache.
    pub fn with_rc() -> RcConstantCache {
        Rc::new(RefCell::new(ConstantCache::new()))
    }

    /// Inserts a new constant into the cache.
    pub fn insert(&mut self, name: String, value: RcObjectType) {
        let mut constants = self.constants.write().unwrap();

        constants.insert(name, value);
    }

    /// Looks up a constant from the cache.
    pub fn get(&mut self, name: &String) -> Option<RcObjectType> {
        let constants = self.constants.read().unwrap();

        constants.get(name).cloned()
    }
}
