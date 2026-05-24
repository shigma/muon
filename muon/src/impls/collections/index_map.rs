//! Observer implementation for [`IndexMap<K, V>`].

use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};

use cfg_version::cfg_version;
use indexmap::map::Entry;
use indexmap::{Equivalent, IndexMap, TryReserveError};
use serde::Serialize;

use crate::general::Snapshot;
use crate::helper::macros::{default_impl_ref_observe, delegate_methods};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{DefaultSpec, Observer, SerializeObserver};
use crate::{MutationKind, Mutations, Observe, PathSegment};

enum ValueState {
    /// Key existed in the original map and was overwritten via
    /// [`insert`](IndexMapObserver::insert).
    Replaced,
    /// Key is new (did not exist in the original map), added via
    /// [`insert`](IndexMapObserver::insert).
    Inserted,
    /// Key existed in the original map and was removed.
    Deleted,
}

struct IndexMapObserverState<K, O> {
    mutated: bool,
    diff: IndexMap<K, ValueState>,
    /// Boxed to ensure pointer stability: [`IndexMap`] rehashing moves all entries to a new
    /// allocation, which would invalidate references to inline values. [`Box`] adds a layer
    /// of indirection so that only the pointer is moved, not the observer itself.
    inner: UnsafeCell<IndexMap<K, Box<O>>>,
}

impl<K, O> Default for IndexMapObserverState<K, O> {
    fn default() -> Self {
        Self {
            mutated: false,
            diff: Default::default(),
            inner: Default::default(),
        }
    }
}

impl<K, O> Invalidate<IndexMap<K, O::Head>> for IndexMapObserverState<K, O>
where
    K: Clone + Eq + Hash,
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
{
    fn invalidate(&mut self, map: &IndexMap<K, O::Head>) {
        if !self.mutated {
            self.mutated = true;
            for key in map.keys() {
                self.mark_deleted(key.clone());
            }
        }
        self.inner.get_mut().clear();
    }
}

impl<K, O> IndexMapObserverState<K, O>
where
    K: Eq + Hash,
{
    fn mark_deleted(&mut self, key: K) {
        self.inner.get_mut().swap_remove(&key);
        match self.diff.entry(key) {
            Entry::Occupied(mut e) => {
                if matches!(e.get(), ValueState::Inserted) {
                    e.swap_remove();
                } else {
                    e.insert(ValueState::Deleted);
                }
            }
            Entry::Vacant(e) => {
                e.insert(ValueState::Deleted);
            }
        }
    }
}

/// Iterator produced by [`IndexMapObserver::extract_if`].
#[cfg_version(indexmap = "2.10")]
pub struct ExtractIf<'a, K, V, O, F>
where
    F: FnMut(&K, &mut V) -> bool,
{
    inner: indexmap::map::ExtractIf<'a, K, V, F>,
    state: Option<&'a mut IndexMapObserverState<K, O>>,
}

#[cfg_version(indexmap = "2.10")]
impl<K, V, O, F> Iterator for ExtractIf<'_, K, V, O, F>
where
    K: Clone + Eq + Hash,
    F: FnMut(&K, &mut V) -> bool,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = self.inner.next()?;
        if let Some(state) = &mut self.state {
            state.mark_deleted(key.clone());
        }
        Some((key, value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[cfg_version(indexmap = "2.10")]
impl<K, V, O, F> FusedIterator for ExtractIf<'_, K, V, O, F>
where
    K: Clone + Eq + Hash,
    F: FnMut(&K, &mut V) -> bool,
{
}

#[cfg_version(indexmap = "2.10")]
impl<K, V, O, F> Debug for ExtractIf<'_, K, V, O, F>
where
    F: FnMut(&K, &mut V) -> bool,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractIf").finish_non_exhaustive()
    }
}

/// Observer implementation for [`IndexMap<K, V>`](IndexMap).
///
/// ## Limitations
///
/// Most methods (e.g. [`insert`](Self::insert), [`swap_remove`](Self::swap_remove),
/// [`get_mut`](Self::get_mut)) require `K: Clone` because the observer maintains its own
/// [`IndexMap`] of cloned keys to track per-key observers independently of the observed map's
/// internal storage.
pub struct IndexMapObserver<K, O, S: ?Sized, D = Zero> {
    ptr: Pointer<S>,
    state: IndexMapObserverState<K, O>,
    phantom: PhantomData<D>,
}

impl<K, O, S: ?Sized, D> Deref for IndexMapObserver<K, O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<K, O, S: ?Sized, D> DerefMut for IndexMapObserver<K, O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.ptr);
        &mut self.ptr
    }
}

impl<K, V, O, S: ?Sized, D> QuasiObserver for IndexMapObserver<K, O, S, D>
where
    K: Clone + Eq + Hash,
    D: Unsigned,
    S: AsDeref<D, Target = IndexMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        Invalidate::invalidate(&mut this.state, (*this.ptr).as_deref());
    }
}

impl<K, O, S: ?Sized, D> Observer for IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Eq + Hash,
{
    fn observe(head: &mut Self::Head) -> Self {
        let this = Self {
            ptr: Pointer::new(head),
            state: Default::default(),
            phantom: PhantomData,
        };
        Pointer::register_state::<_, D>(&this.ptr, &this.state);
        this
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe { Pointer::set_unchecked(this, head) };
    }
}

impl<K, O, S: ?Sized, D> IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero> + SerializeObserver,
    O::Head: Serialize + Sized + 'static,
    K: Serialize + Clone + Eq + Hash + Into<PathSegment> + 'static,
{
    unsafe fn partial_flush(&mut self) -> Mutations {
        let diff = std::mem::take(&mut self.state.diff);
        let mut inner = std::mem::take(self.state.inner.get_mut());
        let mut mutations = Mutations::new();
        for (key, value_state) in diff {
            match value_state {
                ValueState::Deleted => {
                    #[cfg(feature = "delete")]
                    mutations.insert(key, MutationKind::Delete);
                    #[cfg(not(feature = "delete"))]
                    return Mutations::replace((*self).untracked_ref());
                }
                ValueState::Replaced | ValueState::Inserted => {
                    inner.swap_remove(&key);
                    let value = (*self)
                        .untracked_ref()
                        .get(&key)
                        .expect("replaced key not found in observed map");
                    mutations.insert(key, Mutations::replace(value));
                }
            }
        }
        for (key, mut ob) in inner {
            let value = (*self)
                .untracked_ref()
                .get(&key)
                .expect("observer key not found in observed map");
            unsafe { O::relocate(&mut ob, value as *const O::Head as *mut O::Head) }
            mutations.insert(key, unsafe { O::flush(&mut ob) });
        }
        mutations
    }
}

impl<K, O, S: ?Sized, D> SerializeObserver for IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero> + SerializeObserver,
    O::Head: Serialize + Sized + 'static,
    K: Serialize + Clone + Eq + Hash + Into<PathSegment> + 'static,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        if !this.state.mutated {
            return unsafe { this.partial_flush() };
        }
        this.state.mutated = false;
        this.state.diff.clear();
        this.state.inner.get_mut().clear();
        Mutations::replace((*this).untracked_ref())
    }

    unsafe fn flat_flush(this: &mut Self) -> Mutations {
        if !this.state.mutated {
            return unsafe { this.partial_flush() };
        }
        this.state.mutated = false;
        this.state.inner.get_mut().clear();
        // After DerefMut, diff contains only Deleted entries representing original keys.
        // Emit Replace for each current key, Delete for original keys no longer present.
        let mut diff = std::mem::take(&mut this.state.diff);
        let map = (*this.ptr).as_deref();
        let mut mutations = Mutations::new().with_replace(true);
        for (key, value) in map {
            diff.swap_remove(key);
            mutations.insert(key.clone(), Mutations::replace(value));
        }
        for (key, _) in diff {
            #[cfg(feature = "delete")]
            mutations.insert(key, MutationKind::Delete);
            #[cfg(not(feature = "delete"))]
            unreachable!("delete feature is not enabled");
        }
        mutations
    }
}

impl<K, O, S: ?Sized, D, V> IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    K: Clone + Eq + Hash,
{
    delegate_methods! { untracked_mut() as IndexMap =>
        pub fn reserve(&mut self, additional: usize);
        pub fn reserve_exact(&mut self, additional: usize);
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
    }

    delegate_methods! { tracked_mut() as IndexMap =>
        pub fn sort_keys(&mut self) where K: Ord;
        pub fn sort_by<F>(&mut self, cmp: F) where F: FnMut(&K, &V, &K, &V) -> Ordering;
        #[cfg_version(indexmap = "2.11")]
        pub fn sort_by_key<T, F>(&mut self, sort_key: F) where T: Ord, F: FnMut(&K, &V) -> T;
        pub fn sort_unstable_keys(&mut self) where K: Ord;
        pub fn sort_unstable_by<F>(&mut self, cmp: F) where F: FnMut(&K, &V, &K, &V) -> Ordering;
        #[cfg_version(indexmap = "2.11")]
        pub fn sort_unstable_by_key<T, F>(&mut self, sort_key: F) where T: Ord, F: FnMut(&K, &V) -> T;
        pub fn sort_by_cached_key<T, F>(&mut self, sort_key: F) where T: Ord, F: FnMut(&K, &V) -> T;
        pub fn reverse(&mut self);
        pub fn move_index(&mut self, from: usize, to: usize);
        pub fn swap_indices(&mut self, a: usize, b: usize);
    }
}

impl<K, O, S: ?Sized, D, V> IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    K: Clone + Eq + Hash,
{
    delegate_methods! { tracked_mut() as IndexMap =>
        #[cfg_version(indexmap = "2.2.4")]
        pub fn insert_sorted(&mut self, key: K, value: O::Head) -> (usize, Option<O::Head>) where K: Ord;
        #[cfg_version(indexmap = "2.11")]
        pub fn insert_sorted_by<F>(&mut self, key: K, value: O::Head, cmp: F) -> (usize, Option<O::Head>) where F: FnMut(&K, &O::Head, &K, &O::Head) -> Ordering;
        #[cfg_version(indexmap = "2.11")]
        pub fn insert_sorted_by_key<B, F>(&mut self, key: K, value: O::Head, sort_key: F) -> (usize, Option<O::Head>) where B: Ord, F: FnMut(&K, &O::Head) -> B;
        #[cfg_version(indexmap = "2.5")]
        pub fn insert_before(&mut self, index: usize, key: K, value: O::Head) -> (usize, Option<O::Head>);
        #[cfg_version(indexmap = "2.2.3")]
        pub fn shift_insert(&mut self, index: usize, key: K, value: O::Head) -> Option<O::Head>;
        #[cfg_version(indexmap = "2.11")]
        pub fn replace_index(&mut self, index: usize, key: K) -> Result<K, (usize, K)>;
    }
}

impl<K, O, S: ?Sized, D> IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Eq + Hash,
{
    /// See [`IndexMap::get`].
    pub fn get<Q>(&self, key: &Q) -> Option<&O>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        let key_cloned = (*self.ptr).as_deref().get_key_value(key)?.0.clone();
        let value = unsafe { Pointer::as_mut(&self.ptr) }.as_deref_mut().get_mut(key)?;
        match unsafe { (*self.state.inner.get()).entry(key_cloned) } {
            Entry::Occupied(occupied) => {
                let ob = occupied.into_mut().as_mut();
                unsafe { O::relocate(ob, value) }
                Some(ob)
            }
            Entry::Vacant(vacant) => Some(vacant.insert(Box::new(O::observe(value)))),
        }
    }

    /// See [`IndexMap::get_index`].
    pub fn get_index(&self, index: usize) -> Option<(&K, &O)> {
        let key_cloned = (*self.ptr).as_deref().get_index(index)?.0.clone();
        let (key, value) = unsafe { Pointer::as_mut(&self.ptr) }
            .as_deref_mut()
            .get_index_mut(index)?;
        match unsafe { (*self.state.inner.get()).entry(key_cloned) } {
            Entry::Occupied(occupied) => {
                let ob = occupied.into_mut().as_mut();
                unsafe { O::relocate(ob, value) }
                Some((key, ob))
            }
            Entry::Vacant(vacant) => Some((key, vacant.insert(Box::new(O::observe(value))))),
        }
    }
}

impl<K, O, S: ?Sized, D> IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero> + SerializeObserver,
    O::Head: Sized,
    K: Clone + Eq + Hash,
{
    fn replacing_mut(&mut self) -> &mut IndexMap<K, O::Head> {
        self.state.inner.get_mut().clear();
        if (*self).untracked_ref().is_empty() {
            self.untracked_mut()
        } else {
            self.tracked_mut()
        }
    }

    delegate_methods! { replacing_mut() as IndexMap =>
        pub fn clear(&mut self);
    }
}

impl<K, O, S: ?Sized, D> IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Eq + Hash,
{
    fn __force_all(&mut self) -> &mut IndexMap<K, Box<O>> {
        let map = (*self.ptr).as_deref_mut();
        let inner = self.state.inner.get_mut();
        for (key, value) in map.iter_mut() {
            match inner.entry(key.clone()) {
                Entry::Occupied(occupied) => {
                    let observer = occupied.into_mut().as_mut();
                    unsafe { O::relocate(observer, value) }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(Box::new(O::observe(value)));
                }
            }
        }
        inner
    }

    /// See [`IndexMap::iter_mut`].
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut O)> + '_ {
        self.__force_all().iter_mut().map(|(k, v)| (k, v.as_mut()))
    }

    /// See [`IndexMap::values_mut`].
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut O> + '_ {
        self.__force_all().values_mut().map(|v| v.as_mut())
    }

    /// See [`IndexMap::truncate`].
    pub fn truncate(&mut self, len: usize) {
        if self.state.mutated {
            (*self.ptr).as_deref_mut().truncate(len);
            return;
        }
        let map = (*self.ptr).as_deref_mut();
        for key in map.keys().skip(len).cloned() {
            self.state.mark_deleted(key);
        }
        map.truncate(len);
    }

    // TODO
    /// See [`IndexMap::drain`].
    pub fn drain<R>(&mut self, range: R) -> indexmap::map::Drain<'_, K, O::Head>
    where
        R: RangeBounds<usize>,
    {
        if self.state.mutated {
            return (*self.ptr).as_deref_mut().drain(range);
        }
        let map = (*self.ptr).as_deref_mut();
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded => map.len(),
        };
        let keys: Vec<K> = map
            .keys()
            .skip(start)
            .take(end.saturating_sub(start))
            .cloned()
            .collect();
        let drain = map.drain(range);
        for key in keys {
            self.state.mark_deleted(key);
        }
        drain
    }

    /// See [`IndexMap::extract_if`].
    #[cfg_version(indexmap = "2.10")]
    pub fn extract_if<F, R>(&mut self, range: R, pred: F) -> ExtractIf<'_, K, O::Head, O, F>
    where
        R: RangeBounds<usize>,
        F: FnMut(&K, &mut O::Head) -> bool,
    {
        let inner = (*self.ptr).as_deref_mut().extract_if(range, pred);
        let state = if self.state.mutated {
            None
        } else {
            Some(&mut self.state)
        };
        ExtractIf { inner, state }
    }

    /// See [`IndexMap::split_off`].
    pub fn split_off(&mut self, at: usize) -> IndexMap<K, O::Head> {
        if self.state.mutated {
            return self.tracked_mut().split_off(at);
        }
        let split = (*self.ptr).as_deref_mut().split_off(at);
        for key in split.keys().cloned() {
            self.state.mark_deleted(key);
        }
        split
    }

    /// See [`IndexMap::insert`].
    pub fn insert(&mut self, key: K, value: O::Head) -> Option<O::Head> {
        self.insert_full(key, value).1
    }

    /// See [`IndexMap::insert_full`].
    pub fn insert_full(&mut self, key: K, value: O::Head) -> (usize, Option<O::Head>) {
        if self.state.mutated {
            return self.tracked_mut().insert_full(key, value);
        }
        let key_cloned = key.clone();
        let (index, old_value) = (*self.ptr).as_deref_mut().insert_full(key_cloned, value);
        self.state.inner.get_mut().swap_remove(&key);
        match self.state.diff.entry(key) {
            Entry::Occupied(mut e) => {
                if matches!(e.get(), ValueState::Deleted) {
                    e.insert(ValueState::Replaced);
                }
            }
            Entry::Vacant(e) => {
                if old_value.is_some() {
                    e.insert(ValueState::Replaced);
                } else {
                    e.insert(ValueState::Inserted);
                }
            }
        }
        (index, old_value)
    }

    // TODO
    /// See [`IndexMap::splice`].
    #[cfg_version(indexmap = "2.2")]
    pub fn splice<R, I>(&mut self, range: R, replace_with: I) -> std::vec::IntoIter<(K, O::Head)>
    where
        R: RangeBounds<usize>,
        I: IntoIterator<Item = (K, O::Head)>,
    {
        if self.state.mutated {
            return self
                .tracked_mut()
                .splice(range, replace_with)
                .collect::<Vec<_>>()
                .into_iter();
        }
        let map = (*self.ptr).as_deref_mut();
        let replace_with: Vec<_> = replace_with.into_iter().collect();
        let new_keys: Vec<K> = replace_with.iter().map(|(k, _)| k.clone()).collect();

        // Snapshot existing keys to distinguish Insert vs Replace for replacement keys.
        let keys_before: IndexMap<K, ()> = map.keys().cloned().map(|k| (k, ())).collect();
        let removed: Vec<_> = map.splice(range, replace_with).collect();

        // Mark removed keys that are no longer in the map
        for (key, _) in &removed {
            if !map.contains_key(key) {
                self.state.mark_deleted(key.clone());
            }
        }

        // Mark replacement keys as inserted or replaced
        for key in new_keys {
            self.state.inner.get_mut().swap_remove(&key);
            match self.state.diff.entry(key) {
                Entry::Occupied(mut e) => {
                    if matches!(e.get(), ValueState::Deleted) {
                        e.insert(ValueState::Replaced);
                    }
                }
                Entry::Vacant(e) => {
                    if keys_before.contains_key(e.key()) {
                        e.insert(ValueState::Replaced);
                    } else {
                        e.insert(ValueState::Inserted);
                    }
                }
            }
        }
        removed.into_iter()
    }

    /// See [`IndexMap::append`].
    #[cfg_version(indexmap = "2.4")]
    pub fn append(&mut self, other: &mut IndexMap<K, O::Head>) {
        self.extend(other.drain(..))
    }

    /// See [`IndexMap::get_mut`].
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut O>
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        self.get_full_mut(key).map(|(_, _, v)| v)
    }

    /// See [`IndexMap::get_key_value_mut`].
    pub fn get_key_value_mut<Q>(&mut self, key: &Q) -> Option<(&K, &mut O)>
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        self.get_full_mut(key).map(|(_, k, v)| (k, v))
    }

    /// See [`IndexMap::get_full_mut`].
    pub fn get_full_mut<Q>(&mut self, key: &Q) -> Option<(usize, &K, &mut O)>
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        let key_cloned = (*self.ptr).as_deref().get_full(key)?.1.clone();
        let (index, key, value) = (*self.ptr).as_deref_mut().get_full_mut(key)?;
        match self.state.inner.get_mut().entry(key_cloned) {
            Entry::Occupied(occupied) => {
                let ob = occupied.into_mut().as_mut();
                unsafe { O::relocate(ob, value) }
                Some((index, key, ob))
            }
            Entry::Vacant(vacant) => Some((index, key, vacant.insert(Box::new(O::observe(value))))),
        }
    }

    // TODO: get_disjoint_mut

    /// See [`IndexMap::swap_remove`].
    pub fn swap_remove<Q>(&mut self, key: &Q) -> Option<O::Head>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        self.swap_remove_full(key).map(|(_, _, v)| v)
    }

    /// See [`IndexMap::swap_remove_entry`].
    pub fn swap_remove_entry<Q>(&mut self, key: &Q) -> Option<(K, O::Head)>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        self.swap_remove_full(key).map(|(_, k, v)| (k, v))
    }

    /// See [`IndexMap::swap_remove_full`].
    pub fn swap_remove_full<Q>(&mut self, key: &Q) -> Option<(usize, K, O::Head)>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        if self.state.mutated {
            return self.tracked_mut().swap_remove_full(key);
        }
        let (index, key, old_value) = (*self.ptr).as_deref_mut().swap_remove_full(key)?;
        self.state.mark_deleted(key.clone());
        Some((index, key, old_value))
    }

    /// See [`IndexMap::shift_remove`].
    pub fn shift_remove<Q>(&mut self, key: &Q) -> Option<O::Head>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        self.shift_remove_full(key).map(|(_, _, v)| v)
    }

    /// See [`IndexMap::shift_remove_entry`].
    pub fn shift_remove_entry<Q>(&mut self, key: &Q) -> Option<(K, O::Head)>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        self.shift_remove_full(key).map(|(_, k, v)| (k, v))
    }

    /// See [`IndexMap::shift_remove_full`].
    pub fn shift_remove_full<Q>(&mut self, key: &Q) -> Option<(usize, K, O::Head)>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        if self.state.mutated {
            return self.tracked_mut().shift_remove_full(key);
        }
        let (index, key, old_value) = (*self.ptr).as_deref_mut().shift_remove_full(key)?;
        self.state.mark_deleted(key.clone());
        Some((index, key, old_value))
    }

    /// See [`IndexMap::pop`].
    pub fn pop(&mut self) -> Option<(K, O::Head)> {
        if self.state.mutated {
            return self.tracked_mut().pop();
        }
        let (key, old_value) = (*self.ptr).as_deref_mut().pop()?;
        self.state.mark_deleted(key.clone());
        Some((key, old_value))
    }

    /// See [`IndexMap::retain`].
    #[cfg_version(indexmap = "2.10")]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &mut O::Head) -> bool,
    {
        self.extract_if(.., |k, v| !f(k, v)).for_each(drop);
    }

    // TODO: as_mut_slice

    /// See [`IndexMap::get_index_mut`].
    pub fn get_index_mut(&mut self, index: usize) -> Option<(&K, &mut O)> {
        let key_cloned = (*self.ptr).as_deref().get_index(index)?.0.clone();
        let (key, value) = (*self.ptr).as_deref_mut().get_index_mut(index)?;
        match self.state.inner.get_mut().entry(key_cloned) {
            Entry::Occupied(occupied) => {
                let ob = occupied.into_mut().as_mut();
                unsafe { O::relocate(ob, value) }
                Some((key, ob))
            }
            Entry::Vacant(vacant) => Some((key, vacant.insert(Box::new(O::observe(value))))),
        }
    }

    // TODO: get_index_entry
    // TODO: get_disjoint_indices_mut
    // TODO: get_range_mut

    /// See [`IndexMap::first_mut`].
    pub fn first_mut(&mut self) -> Option<(&K, &mut O)> {
        self.get_index_mut(0)
    }

    // TODO: first_entry

    /// See [`IndexMap::last_mut`].
    pub fn last_mut(&mut self) -> Option<(&K, &mut O)> {
        let last = (*self.ptr).as_deref().len().checked_sub(1)?;
        self.get_index_mut(last)
    }

    // TODO: last_entry

    /// See [`IndexMap::swap_remove_index`].
    pub fn swap_remove_index(&mut self, index: usize) -> Option<(K, O::Head)> {
        if self.state.mutated {
            return self.tracked_mut().swap_remove_index(index);
        }
        let (key, old_value) = (*self.ptr).as_deref_mut().swap_remove_index(index)?;
        self.state.mark_deleted(key.clone());
        Some((key, old_value))
    }

    /// See [`IndexMap::shift_remove_index`].
    pub fn shift_remove_index(&mut self, index: usize) -> Option<(K, O::Head)> {
        if self.state.mutated {
            return self.tracked_mut().shift_remove_index(index);
        }
        let (key, old_value) = (*self.ptr).as_deref_mut().shift_remove_index(index)?;
        self.state.mark_deleted(key.clone());
        Some((key, old_value))
    }
}

impl<K, V, O, S: ?Sized, D> Debug for IndexMapObserver<K, O, S, D>
where
    K: Clone + Eq + Hash,
    D: Unsigned,
    S: AsDeref<D, Target = IndexMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    IndexMap<K, V>: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("IndexMapObserver").field(&self.untracked_ref()).finish()
    }
}

impl<K, V, O, S: ?Sized, D> PartialEq<IndexMap<K, V>> for IndexMapObserver<K, O, S, D>
where
    K: Clone + Eq + Hash,
    D: Unsigned,
    S: AsDeref<D, Target = IndexMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    IndexMap<K, V>: PartialEq,
{
    fn eq(&self, other: &IndexMap<K, V>) -> bool {
        self.untracked_ref().eq(other)
    }
}

impl<K1, K2, V1, V2, O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<IndexMapObserver<K2, O2, S2, D2>>
    for IndexMapObserver<K1, O1, S1, D1>
where
    K1: Clone + Eq + Hash,
    K2: Clone + Eq + Hash,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1, Target = IndexMap<K1, V1>>,
    S2: AsDeref<D2, Target = IndexMap<K2, V2>>,
    O1: Observer<InnerDepth = Zero, Head = V1>,
    O2: Observer<InnerDepth = Zero, Head = V2>,
    IndexMap<K1, V1>: PartialEq<IndexMap<K2, V2>>,
{
    fn eq(&self, other: &IndexMapObserver<K2, O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<K, V, O, S: ?Sized, D> Eq for IndexMapObserver<K, O, S, D>
where
    K: Clone + Eq + Hash,
    D: Unsigned,
    S: AsDeref<D, Target = IndexMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    IndexMap<K, V>: Eq,
{
}

impl<'q, K, O, S: ?Sized, D, V, Q: ?Sized> Index<&'q Q> for IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    K: Clone + Eq + Hash,
    Q: Hash + Equivalent<K>,
{
    type Output = O;

    fn index(&self, index: &'q Q) -> &Self::Output {
        self.get(index).expect("no entry found for key")
    }
}

impl<'q, K, O, S: ?Sized, D, V, Q: ?Sized> IndexMut<&'q Q> for IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    K: Clone + Eq + Hash,
    Q: Hash + Equivalent<K>,
{
    fn index_mut(&mut self, index: &'q Q) -> &mut Self::Output {
        self.get_mut(index).expect("no entry found for key")
    }
}

impl<K, O, S: ?Sized, D> Index<usize> for IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Eq + Hash,
{
    type Output = O;

    fn index(&self, index: usize) -> &Self::Output {
        self.get_index(index).expect("index out of bounds").1
    }
}

impl<K, O, S: ?Sized, D> IndexMut<usize> for IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Eq + Hash,
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_index_mut(index).expect("index out of bounds").1
    }
}

// TODO: this inserts elements one by one, which is much slower than `IndexMap::extend`.
// Consider a bulk-insert approach that updates `diff` in one pass.
impl<K, O, S: ?Sized, D> Extend<(K, O::Head)> for IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Eq + Hash,
{
    fn extend<I: IntoIterator<Item = (K, O::Head)>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        let additional = if (*self).untracked_ref().is_empty() {
            iter.size_hint().0
        } else {
            iter.size_hint().0.div_ceil(2)
        };
        self.reserve(additional);
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<'a, K, O, S: ?Sized, D> Extend<(&'a K, &'a O::Head)> for IndexMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Copy,
    K: Copy + Eq + Hash,
{
    fn extend<I: IntoIterator<Item = (&'a K, &'a O::Head)>>(&mut self, iter: I) {
        self.extend(iter.into_iter().map(|(&key, &value)| (key, value)));
    }
}

impl<K: Clone + Eq + Hash, V: Observe> Observe for IndexMap<K, V> {
    type Observer<'ob, S, D>
        = IndexMapObserver<K, V::Observer<'ob, V, Zero>, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ref_observe! {
    impl [K, V] RefObserve for IndexMap<K, V>;
}

impl<K, V> Snapshot for IndexMap<K, V>
where
    K: Snapshot,
    K::Snapshot: Eq + Hash,
    V: Snapshot,
{
    type Snapshot = IndexMap<K::Snapshot, V::Snapshot>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.iter()
            .map(|(key, value)| (key.to_snapshot(), value.to_snapshot()))
            .collect()
    }

    fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
        self.len() == snapshot.len()
            && self
                .iter()
                .zip(snapshot.iter())
                .all(|((key_a, value_a), (key_b, value_b))| key_a.eq_snapshot(key_b) && value_a.eq_snapshot(value_b))
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use muon_test_utils::*;
    use serde_json::json;

    use crate::MutationKind;
    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn remove_nonexistent_key() {
        let mut map = IndexMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        assert_eq!(ob.shift_remove("nonexistent"), None);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn insert_then_remove() {
        let mut map = IndexMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        assert_eq!(ob.insert("b", "y".to_string()), None);
        assert_eq!(ob.shift_remove("b"), Some("y".to_string()));
        assert_eq!(ob.untracked_ref().len(), 1);
        assert_eq!(ob.untracked_ref().get("a"), Some(&"x".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn remove_then_insert() {
        let mut map = IndexMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        assert_eq!(ob.shift_remove("a"), Some("x".to_string()));
        assert_eq!(ob.insert("a", "y".to_string()), None);
        assert_eq!(ob.untracked_ref().get("a"), Some(&"y".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!("y"))));
    }

    #[test]
    fn swap_remove() {
        let mut map = IndexMap::from([("a", "x".to_string()), ("b", "y".to_string()), ("c", "z".to_string())]);
        let mut ob = map.__observe();
        // swap_remove "a" swaps it with the last element "c"
        assert_eq!(ob.swap_remove("a"), Some("x".to_string()));
        assert_eq!(ob.untracked_ref().len(), 2);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(a)));
    }

    #[test]
    fn shift_remove_entry() {
        let mut map = IndexMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
        let mut ob = map.__observe();
        assert_eq!(ob.shift_remove_entry("a"), Some(("a", "x".to_string())));
        assert_eq!(ob.untracked_ref().len(), 1);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(a)));
    }

    #[test]
    fn retain() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        ob.retain(|_, v| *v % 2 != 0);
        assert_eq!(ob.untracked_ref(), &IndexMap::from([("a", 1), ("c", 3)]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(b)));
    }

    #[test]
    fn extend() {
        let mut map = IndexMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        ob.extend([("b", "y".to_string()), ("c", "z".to_string())]);
        assert_eq!(ob.untracked_ref().len(), 3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
    }

    #[test]
    fn get_mut_then_insert() {
        let mut map = IndexMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        ob.get_mut("a").unwrap().push_str(" world");
        ob.insert("a", "bye".to_string());
        assert_eq!(ob.untracked_ref().get("a"), Some(&"bye".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!("bye"))));
    }

    #[test]
    fn insert_then_get_mut() {
        let mut map = IndexMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        ob.insert("b", "hello".to_string());
        ob.get_mut("b").unwrap().push_str(" world");
        assert_eq!(ob.untracked_ref().get("b"), Some(&"hello world".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(b, json!("hello world"))));
    }

    #[test]
    fn iter_mut() {
        let mut map = IndexMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
        let mut ob = map.__observe();
        for (_, v) in ob.iter_mut() {
            v.push_str("!");
        }
        assert_eq!(ob.untracked_ref().get("a"), Some(&"x!".to_string()));
        assert_eq!(ob.untracked_ref().get("b"), Some(&"y!".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
        if let MutationKind::Batch(batch) = mutation.kind {
            assert_eq!(batch.len(), 2);
            for m in &batch {
                assert_eq!(m.kind, MutationKind::Append(json!("!")));
            }
        }
    }

    #[test]
    fn pop() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        assert_eq!(ob.pop(), Some(("c", 3)));
        assert_eq!(ob.untracked_ref(), &IndexMap::from([("a", 1), ("b", 2)]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(c)));
    }

    #[test]
    fn insert_then_pop() {
        let mut map: IndexMap<&str, i32> = IndexMap::new();
        let mut ob = map.__observe();
        ob.insert("a", 1);
        ob.insert("b", 2);
        assert_eq!(ob.pop(), Some(("b", 2)));
        // "b" was inserted then popped: net no-op
        // "a" was inserted: Inserted
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!(1))));
    }

    #[test]
    fn extract_if() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3), ("d", 4)]);
        let mut ob = map.__observe();
        let extracted: IndexMap<_, _> = ob.extract_if(.., |_, v| *v % 2 == 0).collect();
        assert_eq!(extracted, IndexMap::from([("b", 2), ("d", 4)]));
        assert_eq!(ob.untracked_ref(), &IndexMap::from([("a", 1), ("c", 3)]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
    }

    #[test]
    fn extract_if_insert_then_extract() {
        let mut map = IndexMap::from([("a", 1i32)]);
        let mut ob = map.__observe();
        ob.insert("b", 2);
        // extract "b" which was just inserted: net no-op
        let extracted: IndexMap<_, _> = ob.extract_if(.., |k, _| *k == "b").collect();
        assert_eq!(extracted, IndexMap::from([("b", 2)]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn extract_if_with_range() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3), ("d", 4)]);
        let mut ob = map.__observe();
        // Only extract from indices 1..3 ("b" and "c")
        let extracted: IndexMap<_, _> = ob.extract_if(1..3, |_, _| true).collect();
        assert_eq!(extracted, IndexMap::from([("b", 2), ("c", 3)]));
        assert_eq!(ob.untracked_ref(), &IndexMap::from([("a", 1), ("d", 4)]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
    }

    #[test]
    fn index_by_usize() {
        let mut map = IndexMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
        let ob = map.__observe();
        assert_eq!(ob[0].untracked_ref(), "x");
        assert_eq!(ob[1].untracked_ref(), "y");
    }

    #[test]
    fn index_mut_by_usize() {
        let mut map = IndexMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
        let mut ob = map.__observe();
        ob[0].push_str("!");
        ob[1].push_str("?");
        assert_eq!(ob.untracked_ref().get("a"), Some(&"x!".to_string()));
        assert_eq!(ob.untracked_ref().get("b"), Some(&"y?".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
        if let MutationKind::Batch(batch) = mutation.kind {
            assert_eq!(batch.len(), 2);
            assert_eq!(batch[0].kind, MutationKind::Append(json!("!")));
            assert_eq!(batch[1].kind, MutationKind::Append(json!("?")));
        }
    }

    #[test]
    fn values_mut() {
        let mut map = IndexMap::from([("a", "hello".to_string()), ("b", "world".to_string())]);
        let mut ob = map.__observe();
        for v in ob.values_mut() {
            v.push('~');
        }
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
        if let MutationKind::Batch(batch) = mutation.kind {
            assert_eq!(batch.len(), 2);
            for m in &batch {
                assert_eq!(m.kind, MutationKind::Append(json!("~")));
            }
        }
    }

    #[test]
    fn truncate() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3), ("d", 4)]);
        let mut ob = map.__observe();
        ob.truncate(2);
        assert_eq!(ob.untracked_ref(), &IndexMap::from([("a", 1), ("b", 2)]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
        if let MutationKind::Batch(batch) = &mutation.kind {
            assert_eq!(batch.len(), 2);
        }
    }

    #[test]
    fn truncate_noop() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.truncate(5); // len is 2, truncating to 5 is a no-op
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn drain() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3), ("d", 4)]);
        let mut ob = map.__observe();
        let drained: IndexMap<_, _> = ob.drain(1..3).collect();
        assert_eq!(drained, IndexMap::from([("b", 2), ("c", 3)]));
        assert_eq!(ob.untracked_ref(), &IndexMap::from([("a", 1), ("d", 4)]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
    }

    #[test]
    fn drain_all() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        let drained: Vec<_> = ob.drain(..).collect();
        assert_eq!(drained, vec![("a", 1), ("b", 2)]);
        assert!(ob.untracked_ref().is_empty());
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
    }

    #[test]
    fn append_from_other() {
        let mut map = IndexMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        let mut other = IndexMap::from([("b", "y".to_string()), ("c", "z".to_string())]);
        ob.append(&mut other);
        assert!(other.is_empty());
        assert_eq!(ob.untracked_ref().len(), 3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
    }

    #[test]
    fn split_off() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        let split = ob.split_off(1);
        assert_eq!(split, IndexMap::from([("b", 2), ("c", 3)]));
        assert_eq!(ob.untracked_ref(), &IndexMap::from([("a", 1)]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
    }

    #[test]
    fn swap_remove_full() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        assert_eq!(ob.swap_remove_full("b"), Some((1, "b", 2)));
        assert_eq!(ob.untracked_ref().len(), 2);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(b)));
    }

    #[test]
    fn shift_remove_full() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        assert_eq!(ob.shift_remove_full("a"), Some((0, "a", 1)));
        assert_eq!(ob.untracked_ref().len(), 2);
        // Order preserved: b, c
        assert_eq!(ob.untracked_ref().get_index(0), Some((&"b", &2)));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(a)));
    }

    #[test]
    fn get_full_mut() {
        let mut map = IndexMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
        let mut ob = map.__observe();
        let (index, key, value) = ob.get_full_mut("b").unwrap();
        assert_eq!(index, 1);
        assert_eq!(*key, "b");
        value.push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(b, json!("!"))));
    }

    #[test]
    fn first_mut() {
        let mut map = IndexMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
        let mut ob = map.__observe();
        let (key, value) = ob.first_mut().unwrap();
        assert_eq!(*key, "a");
        value.push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(a, json!("!"))));
    }

    #[test]
    fn last_mut() {
        let mut map = IndexMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
        let mut ob = map.__observe();
        let (key, value) = ob.last_mut().unwrap();
        assert_eq!(*key, "b");
        value.push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(b, json!("!"))));
    }

    #[test]
    fn last_mut_empty() {
        let mut map: IndexMap<&str, String> = IndexMap::new();
        let mut ob = map.__observe();
        assert!(ob.last_mut().is_none());
    }

    #[test]
    fn splice_replace_range() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3), ("d", 4)]);
        let mut ob = map.__observe();
        let removed: Vec<_> = ob.splice(1..3, [("x", 10), ("y", 20)]).collect();
        assert_eq!(removed, vec![("b", 2), ("c", 3)]);
        // Final order: a, x, y, d
        assert_eq!(ob.untracked_ref().get("a"), Some(&1));
        assert_eq!(ob.untracked_ref().get("x"), Some(&10));
        assert_eq!(ob.untracked_ref().get("y"), Some(&20));
        assert_eq!(ob.untracked_ref().get("d"), Some(&4));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
        if let MutationKind::Batch(batch) = &mutation.kind {
            // b: Delete, c: Delete, x: Insert, y: Insert
            assert_eq!(batch.len(), 4);
        }
    }

    #[test]
    fn splice_reinsert_key() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        // Remove "b" and re-insert "b" with a new value
        let removed: Vec<_> = ob.splice(1..2, [("b", 20)]).collect();
        assert_eq!(removed, vec![("b", 2)]);
        assert_eq!(ob.untracked_ref().get("b"), Some(&20));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(b, json!(20))));
    }

    #[test]
    fn extend_ref() {
        let mut map = IndexMap::from([("a", 1i32)]);
        let mut ob = map.__observe();
        ob.extend([(&"b", &2), (&"c", &3)]);
        assert_eq!(ob.untracked_ref().len(), 3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
    }

    #[test]
    fn flat_flush_no_change() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn flat_flush_deref_mut_only() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        *ob.tracked_mut() = IndexMap::from([("a", 10), ("b", 20)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(a, json!(10)), replace!(b, json!(20))))
        );
    }

    // Inserted key, then deref_mut to a value without that key -> no Delete for the inserted key
    #[test]
    fn flat_flush_inserted_then_absent() {
        let mut map = IndexMap::from([("a", 1i32)]);
        let mut ob = map.__observe();
        ob.insert("b", 2);
        *ob.tracked_mut() = IndexMap::from([("a", 10)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!(10))));
    }

    // Inserted key, then deref_mut to a value with that key -> Replace for the key
    #[test]
    fn flat_flush_inserted_then_present() {
        let mut map = IndexMap::from([("a", 1i32)]);
        let mut ob = map.__observe();
        ob.insert("b", 2);
        *ob.tracked_mut() = IndexMap::from([("a", 10), ("b", 20)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(a, json!(10)), replace!(b, json!(20))))
        );
    }

    // Deleted key, then deref_mut to a value without that key -> Delete for the key
    #[test]
    fn flat_flush_deleted_then_absent() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.shift_remove("b");
        *ob.tracked_mut() = IndexMap::from([("a", 10)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, replace!(a, json!(10)), delete!(b))));
    }

    // Deleted key, then deref_mut to a value with that key -> Replace (not Delete)
    #[test]
    fn flat_flush_deleted_then_present() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.shift_remove("b");
        *ob.tracked_mut() = IndexMap::from([("a", 10), ("b", 20)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(a, json!(10)), replace!(b, json!(20))))
        );
    }

    // Replaced key, then deref_mut to a value without that key -> Delete for the key
    #[test]
    fn flat_flush_replaced_then_absent() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.insert("b", 99);
        *ob.tracked_mut() = IndexMap::from([("a", 10)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, replace!(a, json!(10)), delete!(b))));
    }

    // Replaced key, then deref_mut to a value with that key -> Replace
    #[test]
    fn flat_flush_replaced_then_present() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.insert("b", 99);
        *ob.tracked_mut() = IndexMap::from([("a", 10), ("b", 20)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(a, json!(10)), replace!(b, json!(20))))
        );
    }

    // Without deref_mut, flat_flush returns granular mutations with is_replace=false
    #[test]
    fn flat_flush_granular() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.insert("a", 10);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!(10))));
    }

    // deref_mut replaces with entirely new keys
    #[test]
    fn flat_flush_deref_mut_new_keys() {
        let mut map = IndexMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        *ob.tracked_mut() = IndexMap::from([("c", 30)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(c, json!(30)), delete!(a), delete!(b)))
        );
    }
}
