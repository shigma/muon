use std::fmt::Debug;

use erased_serde::Serialize;

use crate::{Path, PathSegment};

/// The kind of mutation that occurred.
///
/// [`MutationKind`] represents the specific type of change made to a value. Different kinds enable
/// optimizations and more precise change descriptions.
///
/// ## Variants
///
/// - [`Replace`](MutationKind::Replace): Complete replacement of a value
/// - [`Append`](MutationKind::Append): Append operation for strings and vectors
/// - [`Truncate`](MutationKind::Truncate): Truncate operation for strings and vectors
/// - [`Delete`](MutationKind::Delete): Deletion of a value from a map or conditional skip
/// - [`Batch`](MutationKind::Batch): Multiple mutations combined
///
/// ## Example
///
/// ```
/// use muon::adapter::Json;
/// use muon::{Mutation, MutationKind, Observe, observe};
/// use serde::Serialize;
/// use serde_json::json;
///
/// #[derive(Serialize, Observe)]
/// struct Document {
///     title: String,
///     content: String,
///     tags: Vec<String>,
/// }
///
/// let mut doc = Document {
///     title: "Draft".to_string(),
///     content: "Hello".to_string(),
///     tags: vec!["todo".to_string()],
/// };
///
/// let Json(mutation) = observe!(doc => {
///     doc.title = "Final".to_string();      // Replace
///     doc.content.push_str(" World");       // Append
///     doc.tags.push("done".to_string());    // Append
/// }).unwrap();
///
/// // The mutation contains a Batch with three kinds
/// assert!(matches!(mutation.unwrap().kind, MutationKind::Batch(_)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationKind<T> {
    /// [`Replace`](MutationKind::Replace) is the default mutation for
    /// [`DerefMut`](std::ops::DerefMut) operations.
    ///
    /// ## Examples
    ///
    /// ```
    /// # #[derive(Default)]
    /// # struct Foo {
    /// #   a: A,
    /// #   num: i32,
    /// #   vec: Vec<i32>,
    /// # }
    /// # #[derive(Default)]
    /// # struct A {
    /// #   b: i32,
    /// # }
    /// # let mut foo = Foo::default();
    /// foo.a.b = 1;        // Replace at .a.b
    /// foo.num *= 2;       // Replace at .num
    /// foo.vec.clear();    // Replace at .vec
    /// ```
    Replace(T),

    /// [`Append`](MutationKind::Append) represents adding data to the end of a string or vector.
    /// This is more efficient than [`Replace`](MutationKind::Replace) because only the appended
    /// portion needs to be serialized and transmitted.
    ///
    /// ## Examples
    ///
    /// ```
    /// # #[derive(Default)]
    /// # struct Foo {
    /// #   a: A,
    /// #   vec: Vec<i32>,
    /// # }
    /// # #[derive(Default)]
    /// # struct A {
    /// #   b: String,
    /// # }
    /// # let mut foo = Foo::default();
    /// # let iter = vec![2, 3].into_iter();
    /// foo.a.b += "text";          // Append to .a.b
    /// foo.a.b.push_str("text");   // Append to .a.b
    /// foo.vec.push(1);            // Append to .vec
    /// foo.vec.extend(iter);       // Append to .vec
    /// ```
    #[cfg(feature = "append")]
    Append(T),

    /// [`Truncate`](MutationKind::Truncate) represents removing elements from the end of a string
    /// or vector. This is more efficient than [`Replace`](MutationKind::Replace) because only
    /// the truncation length needs to be serialized and transmitted.
    ///
    /// ## Examples
    ///
    /// ```
    /// # #[derive(Default)]
    /// # struct Foo {
    /// #   a: A,
    /// #   vec: Vec<i32>,
    /// # }
    /// # #[derive(Default)]
    /// # struct A {
    /// #   b: String,
    /// # }
    /// let mut foo = Foo {
    ///     a: A { b: "Hello, World!".to_string() },
    ///     vec: vec![1, 2, 3, 4, 5],
    /// };
    /// foo.a.b.truncate(5);        // Truncate 8 chars from .a.b
    /// foo.vec.pop();              // Truncate 1 element from .vec
    #[cfg(feature = "truncate")]
    Truncate(usize),

    /// [`Delete`](MutationKind::Delete) represents the removal of a value entirely.
    ///
    /// This mutation kind is used in two scenarios:
    ///
    /// 1. **Map deletions**: When a key-value pair is removed from a map-like data structure (e.g.,
    ///    [`HashMap::remove`](std::collections::HashMap::remove))
    /// 2. **Conditional serialization skips**: When a value transitions from being serialized to
    ///    being skipped due to conditions like `#[serde(skip_serializing_if)]`
    ///
    /// Unlike [`Replace`](MutationKind::Replace), which updates a value in place, `Delete`
    /// removes the value at the specified path from the parent container entirely.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use std::collections::HashMap;
    /// # #[derive(Default)]
    /// # struct Foo {
    /// #   map: HashMap<String, i32>,
    /// #   value: Option<i32>,
    /// # }
    /// # let mut foo = Foo::default();
    /// foo.map.remove("key");      // Delete at .map.key
    /// // #[serde(skip_serializing_if = "Option::is_none")]
    /// foo.value = None;           // Delete at .value
    /// ```
    #[cfg(feature = "delete")]
    Delete,

    /// [`Batch`](MutationKind::Batch) combines multiple mutations that occurred during a single
    /// observation period. This is automatically created when multiple independent changes are
    /// detected.
    ///
    /// ## Optimization
    ///
    /// The batch collector ([`BatchTree`](crate::BatchTree)) automatically optimizes mutations:
    /// - Consecutive appends are merged
    /// - Redundant changes are eliminated
    /// - Nested paths are consolidated when possible
    Batch(Vec<Mutation<T>>),
}

impl<T> MutationKind<T> {
    #[cfg(any(feature = "json", feature = "yaml"))]
    pub(crate) fn try_map<U, E>(self, f: &mut impl FnMut(T) -> Result<U, E>) -> Result<MutationKind<U>, E> {
        Ok(match self {
            MutationKind::Replace(value) => MutationKind::Replace(f(value)?),
            #[cfg(feature = "append")]
            MutationKind::Append(value) => MutationKind::Append(f(value)?),
            #[cfg(feature = "truncate")]
            MutationKind::Truncate(len) => MutationKind::Truncate(len),
            #[cfg(feature = "delete")]
            MutationKind::Delete => MutationKind::Delete,
            MutationKind::Batch(batch) => {
                MutationKind::Batch(batch.into_iter().map(|m| m.try_map(f)).collect::<Result<_, E>>()?)
            }
        })
    }
}

/// A mutation representing a change to a value at a specific path.
///
/// [`Mutation`] captures both the location where a change occurred (via `path`) and the kind of
/// change that was made (via `kind`). Mutations can be applied to values to reproduce the changes
/// they represent.
///
/// ## Path Representation
///
/// The path is stored in *reverse order* for efficiency during collection.
/// For example, a change at `foo.bar.baz` would have `path = ["baz", "bar", "foo"]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation<V> {
    /// The path to the mutated value, stored in *reverse order*.
    ///
    /// An empty vec indicates a mutation at the root level.
    pub path: Path<true>,

    /// The kind of mutation that occurred.
    pub kind: MutationKind<V>,
}

impl<V> Mutation<V> {
    #[cfg(any(feature = "json", feature = "yaml"))]
    pub(crate) fn try_map<U, E>(self, f: &mut impl FnMut(V) -> Result<U, E>) -> Result<Mutation<U>, E> {
        Ok(Mutation {
            path: self.path,
            kind: self.kind.try_map(f)?,
        })
    }

    fn make_batch(&mut self, capacity: usize) -> &mut Vec<Self> {
        if self.path.is_empty()
            && let MutationKind::Batch(ref mut batch) = self.kind
        {
            return batch;
        }
        let old = std::mem::replace(
            self,
            Mutation {
                path: vec![].into(),
                kind: MutationKind::Batch(Vec::with_capacity(capacity)),
            },
        );
        let MutationKind::Batch(batch) = &mut self.kind else {
            unreachable!()
        };
        batch.push(old);
        batch
    }
}

/// A collection of mutations collected during observation.
///
/// It is the return type for [`flush`](crate::observe::SerializeObserver::flush) and
/// [`flat_flush`](crate::observe::SerializeObserver::flat_flush) operations.
///
/// ## Behavior
///
/// - If no mutations are pushed, [`into_inner`](Mutations::into_inner) returns [`None`].
/// - If exactly one mutation is pushed, it is returned as-is.
/// - If multiple mutations are pushed, they are wrapped in a [`Batch`](MutationKind::Batch).
///
/// The [`is_replace`](Self::is_replace) flag tracks whether this collection represents a
/// whole-value replace. It is set automatically when constructed from a
/// [`Replace`](MutationKind::Replace) mutation, and is used by
/// [`flat_flush`](crate::observe::SerializeObserver::flat_flush) to propagate replace status
/// to parent observers for replace collapse.
///
/// ## Example
///
/// ```
/// use muon::{Mutation, MutationKind, Mutations};
///
/// let mut mutations = Mutations::new();
///
/// mutations.insert("a", MutationKind::Replace(42));
/// mutations.insert("b", MutationKind::Truncate(1));
///
/// let result = mutations.into_inner();
/// assert!(matches!(result, Some(Mutation { kind: MutationKind::Batch(_), .. })));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutations<V = Box<dyn Serialize>> {
    inner: Option<Mutation<V>>,
    capacity: usize,
    is_replace: bool,
}

impl<V> Default for Mutations<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> From<MutationKind<V>> for Mutations<V> {
    fn from(kind: MutationKind<V>) -> Self {
        Self {
            is_replace: matches!(kind, MutationKind::Replace(_)),
            inner: Some(Mutation {
                path: Default::default(),
                kind,
            }),
            capacity: 2,
        }
    }
}

impl<V> From<Mutations<V>> for Option<Mutation<V>> {
    fn from(value: Mutations<V>) -> Self {
        value.into_inner()
    }
}

impl<V> Mutations<V> {
    /// Creates a new empty collection.
    pub fn new() -> Self {
        Self {
            is_replace: false,
            inner: None,
            capacity: 2,
        }
    }

    /// Sets the capacity hint for the internal [`Batch`](MutationKind::Batch) storage.
    ///
    /// The capacity hint is used when the internal storage needs to be converted to a
    /// [`Batch`](MutationKind::Batch) to hold multiple mutations.
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// Sets the [`is_replace`](Self::is_replace) flag on this collection.
    ///
    /// This is used by composite observers to propagate replace status through
    /// [`flat_flush`](crate::observe::SerializeObserver::flat_flush), where the collection may
    /// contain per-field mutations that are a flattened decomposition of a whole-value replace.
    pub fn with_replace(mut self, is_replace: bool) -> Self {
        self.is_replace = is_replace;
        self
    }

    /// Prepends a path segment to the contained mutation.
    ///
    /// If the collection is empty, this is a no-op. Otherwise, the segment is pushed onto the
    /// mutation's reverse-order path.
    pub fn with_prefix(mut self, segment: impl Into<PathSegment>) -> Self {
        if let Some(mutation) = &mut self.inner {
            mutation.path.push(segment.into());
        }
        self
    }

    /// Consumes the batch and returns the collected mutation.
    pub fn into_inner(self) -> Option<Mutation<V>> {
        self.inner
    }

    /// Returns `true` if this collection represents a whole-value replace.
    ///
    /// This flag is set automatically when the collection is created from a
    /// [`Replace`](MutationKind::Replace) mutation kind, or explicitly via
    /// [`with_replace`](Self::with_replace). It is used by
    /// [`flat_flush`](crate::observe::SerializeObserver::flat_flush) to signal to the parent
    /// observer that the entire content was replaced, enabling replace collapse.
    pub fn is_replace(&self) -> bool {
        self.is_replace
    }

    /// Merges another collection of mutations into this one.
    ///
    /// If the incoming collection contains a [`Batch`](MutationKind::Batch) with an empty path, its
    /// inner mutations are flattened into this collection rather than being nested.
    pub fn extend(&mut self, mutations: impl Into<Self>) {
        let Some(incoming) = mutations.into().into_inner() else {
            return;
        };
        let Some(existing) = &mut self.inner else {
            self.inner = Some(incoming);
            return;
        };
        let existing_batch: &mut Vec<Mutation<V>> = existing.make_batch(self.capacity);
        if incoming.path.is_empty()
            && let MutationKind::Batch(incoming_batch) = incoming.kind
        {
            existing_batch.extend(incoming_batch);
        } else {
            existing_batch.push(incoming);
        }
    }

    /// Inserts mutations at a specified path segment.
    ///
    /// The incoming mutations will have the given segment prepended to their path before being
    /// added to this collection.
    pub fn insert(&mut self, segment: impl Into<PathSegment>, mutations: impl Into<Self>) {
        self.extend(mutations.into().with_prefix(segment))
    }

    /// Returns the number of top-level mutations in this collection.
    ///
    /// A top-level mutation is one with an empty path. If this collection contains a
    /// [`Batch`](MutationKind::Batch) with an empty path, this returns the number of mutations
    /// in that batch. Otherwise, it returns `1` if a mutation exists, or `0` if the collection is
    /// empty.
    pub fn len(&self) -> usize {
        match &self.inner {
            None => 0,
            Some(mutation) => match &mutation.kind {
                MutationKind::Batch(batch) if mutation.path.is_empty() => batch.len(),
                _ => 1,
            },
        }
    }

    /// Returns `true` if this collection contains no mutations.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Creates a [`Mutations`] containing a single [`Truncate`](MutationKind::Truncate) mutation.
    #[cfg(feature = "truncate")]
    pub fn truncate(len: usize) -> Self {
        MutationKind::Truncate(len).into()
    }

    /// Creates a [`Mutations`] containing a single [`Delete`](MutationKind::Delete) mutation.
    #[cfg(feature = "delete")]
    pub fn delete() -> Self {
        MutationKind::Delete.into()
    }

    /// Converts all mutations in this collection to [`Delete`](MutationKind::Delete).
    ///
    /// Each mutation retains its path but has its kind replaced with
    /// [`Delete`](MutationKind::Delete). For a [`Batch`](MutationKind::Batch), every inner
    /// mutation is converted individually, preserving the per-field paths. This is used by
    /// [`flat_flush`](crate::observe::SerializeObserver::flat_flush) when the parent needs to emit
    /// deletions for all fields of a flattened struct or map.
    #[cfg(feature = "delete")]
    pub fn into_delete(mut self) -> Self {
        if let Some(mutation) = &mut self.inner {
            match &mut mutation.kind {
                MutationKind::Batch(batch) => {
                    for mutation in batch {
                        mutation.kind = MutationKind::Delete;
                    }
                }
                _ => mutation.kind = MutationKind::Delete,
            }
        }
        self
    }
}

/// A raw-pointer wrapper that implements [`Serialize`](serde::Serialize) for `?Sized` types.
///
/// This type enables creating [`Box<dyn Serialize>`](erased_serde::Serialize) from references to
/// unsized types like `str` and `[T]`, which cannot be directly cast to `&dyn Serialize` because
/// `&dyn Serialize` requires `Sized`. By wrapping the raw pointer in a `Sized` struct, the
/// [`Serialize`](serde::Serialize) implementation can dereference the pointer during serialization.
///
/// ## Safety
///
/// The pointed-to value must remain valid until serialization occurs. This is guaranteed by the
/// observer's `'ob` lifetime — the observed data outlives all mutations produced by
/// [`flush`](crate::observe::SerializeObserver::flush).
pub(crate) struct SerializeRef<T: ?Sized>(pub *const T);

impl<T> serde::Serialize for SerializeRef<T>
where
    T: serde::Serialize + ?Sized,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        unsafe { &*self.0 }.serialize(serializer)
    }
}

impl Mutations {
    /// Creates a [`Mutations`] containing a single [`Replace`](MutationKind::Replace) mutation,
    /// taking ownership of the value.
    ///
    /// Unlike [`replace`](Self::replace), which accepts `&T` (including unsized types) and wraps it
    /// in [`SerializeRef`], this method takes `T` by value and boxes it directly.
    ///
    /// ## Safety (internal)
    ///
    /// Uses `transmute` to erase lifetime from the boxed trait object. This is sound
    /// because the pointed-to value's validity is guaranteed by the observer's `'ob` lifetime —
    /// serialization always completes before the observed data is dropped.
    pub fn replace_owned<T: serde::Serialize>(value: T) -> Self {
        let boxed = unsafe {
            std::mem::transmute::<Box<dyn Serialize + '_>, Box<dyn Serialize>>(Box::new(value))
        };
        MutationKind::Replace(boxed).into()
    }

    /// Creates a [`Mutations`] containing a single [`Replace`](MutationKind::Replace) mutation
    /// with the given value.
    ///
    /// The value is wrapped in a [`Box<dyn Serialize>`](erased_serde::Serialize) via
    /// [`SerializeRef`], allowing unsized types like `str` and `[T]` to be used.
    ///
    /// ## Safety (internal)
    ///
    /// Uses [`SerializeRef`] to store a raw pointer for deferred serialization.
    /// The pointed-to value's validity is guaranteed by the observer's `'ob` lifetime —
    /// serialization always completes before the observed data is dropped.
    pub fn replace<T: serde::Serialize + ?Sized>(value: &T) -> Self {
        Self::replace_owned(SerializeRef(value))
    }

    /// Creates a [`Mutations`] containing a single [`Append`](MutationKind::Append) mutation
    /// with the given value.
    ///
    /// The value is wrapped in a [`Box<dyn Serialize>`](erased_serde::Serialize) via
    /// [`SerializeRef`], allowing unsized types like `str` and `[T]` to be used.
    ///
    /// ## Safety (internal)
    ///
    /// Uses [`SerializeRef`] to store a raw pointer for deferred serialization.
    /// The pointed-to value's validity is guaranteed by the observer's `'ob` lifetime —
    /// serialization always completes before the observed data is dropped.
    #[cfg(feature = "append")]
    pub fn append<T: serde::Serialize + ?Sized>(value: &T) -> Self {
        Self::append_owned(SerializeRef(value))
    }

    /// Creates a [`Mutations`] containing a single [`Append`](MutationKind::Append) mutation,
    /// taking ownership of the value.
    ///
    /// Unlike [`append`](Self::append), which accepts `&T` (including unsized types) and wraps it
    /// in [`SerializeRef`], this method takes `T` by value and boxes it directly.
    ///
    /// ## Safety (internal)
    ///
    /// Uses `transmute` to erase lifetime from the boxed trait object. This is sound
    /// because the pointed-to value's validity is guaranteed by the observer's `'ob` lifetime —
    /// serialization always completes before the observed data is dropped.
    #[cfg(feature = "append")]
    pub fn append_owned<T: serde::Serialize>(value: T) -> Self {
        let boxed = unsafe {
            std::mem::transmute::<Box<dyn Serialize + '_>, Box<dyn Serialize>>(Box::new(value))
        };
        MutationKind::Append(boxed).into()
    }
}
