//! Hash maps and sets that iterate in the same order on every run.
//!
//! The solvers branch in iteration order: the arbitrary heuristic takes the first free literal,
//! the others break ties with it. With std's randomly seeded hasher the same formula took
//! seconds on one run and minutes on the next (the DPLL test on aim-50-1_6-no-1 from 4 s to
//! almost a minute), and a bug could show up on one run in ten. A fixed hasher makes every run
//! on a formula take the same path: the same model, the same decisions, the same time.

use std::hash::{BuildHasherDefault, DefaultHasher};

/// [`DefaultHasher::new`] always uses the same keys, unlike `RandomState`.
pub(crate) type Fixed = BuildHasherDefault<DefaultHasher>;
pub(crate) type HashMap<K, V> = std::collections::HashMap<K, V, Fixed>;
pub(crate) type HashSet<T> = std::collections::HashSet<T, Fixed>;

/// An empty map with room for `capacity` entries.
pub(crate) fn map<K, V>(capacity: usize) -> HashMap<K, V> {
    HashMap::with_capacity_and_hasher(capacity, Fixed::default())
}

/// An empty set with room for `capacity` entries.
pub(crate) fn set<T>(capacity: usize) -> HashSet<T> {
    HashSet::with_capacity_and_hasher(capacity, Fixed::default())
}
