//! Observer implementations for collection types in [`std::collections`].

pub mod binary_heap;
pub mod btree_map;
pub mod btree_set;
pub mod hash_map;
pub mod hash_set;
#[cfg(feature = "indexmap")]
pub mod index_map;
#[cfg(feature = "indexmap")]
pub mod index_set;

pub use binary_heap::BinaryHeapObserver;
pub use btree_map::BTreeMapObserver;
pub use btree_set::BTreeSetObserver;
pub use hash_map::HashMapObserver;
pub use hash_set::HashSetObserver;
#[cfg(feature = "indexmap")]
pub use index_map::IndexMapObserver;
#[cfg(feature = "indexmap")]
pub use index_set::IndexSetObserver;
