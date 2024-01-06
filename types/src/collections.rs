//! Generic collections for the compiler, such as an ordered map.
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::mem::swap;
use std::ops::{Index, IndexMut};

/// A hash map that can be accessed both by a key and an index.
pub struct IndexMap<K: Eq + Hash, V> {
    /// The values stored in this table.
    ///
    /// Values are stored in the same order as they are added in.
    values: Vec<V>,

    /// Mapping of names to their indexes in the table.
    ///
    /// We can't map names to references as this would prevent moving of the
    /// table itself. Using indexes requires extra indirection, but the cost of
    /// this doesn't matter.
    mapping: HashMap<K, usize>,
}

impl<K: Eq + Hash, V> IndexMap<K, V> {
    pub fn new() -> Self {
        Self { values: Vec::new(), mapping: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn insert(&mut self, key: K, value: V) {
        let index = self.values.len();

        self.values.push(value);
        self.mapping.insert(key, index);
    }

    pub fn get<Q: ?Sized>(&self, name: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.mapping.get(name).cloned().and_then(|index| self.values.get(index))
    }

    pub fn get_mut<Q: ?Sized>(&mut self, name: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.mapping
            .get(name)
            .cloned()
            .and_then(|index| self.values.get_mut(index))
    }

    pub fn get_index(&self, index: usize) -> Option<&V> {
        self.values.get(index)
    }

    pub fn take_values(&mut self) -> Vec<V> {
        let mut values = Vec::new();

        swap(&mut values, &mut self.values);
        self.mapping.clear();
        values
    }

    pub fn values(&self) -> &Vec<V> {
        &self.values
    }

    pub fn values_mut(&mut self) -> &mut Vec<V> {
        &mut self.values
    }

    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.mapping.contains_key(k)
    }

    pub fn index_of<Q: ?Sized>(&self, k: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.mapping.get(k).cloned()
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.mapping.keys()
    }
}

impl<K: Eq + Hash, V> Index<usize> for IndexMap<K, V> {
    type Output = V;

    fn index(&self, index: usize) -> &Self::Output {
        &self.values[index]
    }
}

impl<K: Eq + Hash, V> IndexMut<usize> for IndexMap<K, V> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.values[index]
    }
}

impl<K: Clone + Eq + Hash, V: Clone> Clone for IndexMap<K, V> {
    fn clone(&self) -> Self {
        IndexMap { values: self.values.clone(), mapping: self.mapping.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_len() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_insert() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert_eq!(map.values, vec![10]);
        assert_eq!(map.mapping.get("A"), Some(&0));
    }

    #[test]
    fn test_get() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert_eq!(map.get(&"A"), Some(&10));
    }

    #[test]
    fn test_get_mut() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        *map.get_mut(&"A").unwrap() = 20;

        assert_eq!(map.get(&"A"), Some(&20));
    }

    #[test]
    fn test_get_index() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert_eq!(map.get_index(0), Some(&10));
        assert_eq!(map.get_index(1), None);
    }

    #[test]
    fn test_values() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert_eq!(map.values(), &vec![10]);
    }

    #[test]
    fn test_contains_key() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert!(map.contains_key(&"A"));
        assert!(!map.contains_key(&"B"));
    }

    #[test]
    fn test_index() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert_eq!(map[0], 10);
    }

    #[test]
    fn test_index_mut() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        map[0] = 20;

        assert_eq!(map.values[0], 20);
    }

    #[test]
    fn test_clone() {
        let mut map1 = IndexMap::new();

        map1.insert("A", 10);

        let map2 = map1.clone();

        assert_eq!(map2[0], 10);
    }

    #[test]
    fn test_index_of() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert_eq!(map.index_of("A"), Some(0));
        assert_eq!(map.index_of("B"), None);
    }

    #[test]
    fn test_keys() {
        let mut map = IndexMap::new();

        map.insert("A", 10);

        assert_eq!(map.keys().next(), Some(&"A"));
    }
}
