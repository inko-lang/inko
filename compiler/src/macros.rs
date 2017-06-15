#![macro_use]

/// Creates a new HashMap
///
/// Example:
///
///     let hash = hash_map!(key => value);
///
macro_rules! hash_map {
    ( $($key: expr => $value: expr),+ ) => ({
        let mut map = $crate::std::collections::HashMap::new();

        $(map.insert($key, $value);)+

        map
    });
}

macro_rules! hash_set {
    [ $($value: expr),+, ] => ({
        let mut set = $crate::std::collections::HashSet::new();

        $(set.insert($value);)+

        set
    });
}
