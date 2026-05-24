//! Observer implementation for [`BTreeMap<K, V>`].

use std::borrow::Borrow;
use std::cell::UnsafeCell;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt::Debug;
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Index, IndexMut, RangeBounds};

use serde::Serialize;

use crate::general::Snapshot;
use crate::helper::macros::default_impl_ref_observe;
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{DefaultSpec, Observer, SerializeObserver};
use crate::{MutationKind, Mutations, Observe, PathSegment};

enum ValueState {
    /// Key existed in the original map and was overwritten via
    /// [`insert`](BTreeMapObserver::insert).
    Replaced,
    /// Key is new (did not exist in the original map), added via
    /// [`insert`](BTreeMapObserver::insert).
    Inserted,
    /// Key existed in the original map and was removed.
    Deleted,
}

struct BTreeMapObserverState<K, O> {
    mutated: bool,
    diff: BTreeMap<K, ValueState>,
    /// Boxed to ensure pointer stability: [`BTreeMap`] node splits move entries between nodes
    /// via `memcpy`, which would invalidate references to inline values. [`Box`] adds a layer
    /// of indirection so that only the pointer is moved, not the observer itself.
    inner: UnsafeCell<BTreeMap<K, Box<O>>>,
}

impl<K, O> Default for BTreeMapObserverState<K, O> {
    fn default() -> Self {
        Self {
            mutated: false,
            diff: Default::default(),
            inner: Default::default(),
        }
    }
}

impl<K, O> BTreeMapObserverState<K, O>
where
    K: Ord,
{
    fn mark_deleted(&mut self, key: K) {
        self.inner.get_mut().remove(&key);
        match self.diff.entry(key) {
            Entry::Occupied(mut e) => {
                if matches!(e.get(), ValueState::Inserted) {
                    e.remove();
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

impl<K, O> Invalidate<BTreeMap<K, O::Head>> for BTreeMapObserverState<K, O>
where
    K: Clone + Ord,
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
{
    fn invalidate(&mut self, map: &BTreeMap<K, O::Head>) {
        if !self.mutated {
            self.mutated = true;
            for key in map.keys() {
                self.mark_deleted(key.clone());
            }
        }
        self.inner.get_mut().clear();
    }
}

/// Iterator produced by [`BTreeMapObserver::extract_if`].
#[rustversion::since(1.91)]
pub struct ExtractIf<'a, K, V, O, R, F>
where
    R: RangeBounds<K>,
    F: FnMut(&K, &mut V) -> bool,
{
    inner: std::collections::btree_map::ExtractIf<'a, K, V, R, F>,
    state: Option<&'a mut BTreeMapObserverState<K, O>>,
}

#[rustversion::since(1.91)]
impl<K, V, O, R, F> Iterator for ExtractIf<'_, K, V, O, R, F>
where
    K: Clone + Ord,
    R: RangeBounds<K>,
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

#[rustversion::since(1.91)]
impl<K, V, O, R, F> FusedIterator for ExtractIf<'_, K, V, O, R, F>
where
    K: Clone + Ord,
    R: RangeBounds<K>,
    F: FnMut(&K, &mut V) -> bool,
{
}

#[rustversion::since(1.91)]
impl<K, V, O, R, F> Debug for ExtractIf<'_, K, V, O, R, F>
where
    K: Debug,
    V: Debug,
    R: RangeBounds<K>,
    F: FnMut(&K, &mut V) -> bool,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

/// Observer implementation for [`BTreeMap<K, V>`].
///
/// ## Limitations
///
/// Most methods (e.g. [`insert`](Self::insert), [`remove`](Self::remove),
/// [`get_mut`](Self::get_mut)) require `K: Clone` because the observer maintains its own
/// [`BTreeMap`] of cloned keys to track per-key observers independently of the observed map's
/// internal storage.
pub struct BTreeMapObserver<K, O, S: ?Sized, D = Zero> {
    ptr: Pointer<S>,
    state: BTreeMapObserverState<K, O>,
    phantom: PhantomData<D>,
}

impl<K, O, S: ?Sized, D> Deref for BTreeMapObserver<K, O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<K, O, S: ?Sized, D> DerefMut for BTreeMapObserver<K, O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.ptr);
        &mut self.ptr
    }
}

impl<K, V, O, S: ?Sized, D> QuasiObserver for BTreeMapObserver<K, O, S, D>
where
    K: Clone + Ord,
    D: Unsigned,
    S: AsDeref<D, Target = BTreeMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        Invalidate::invalidate(&mut this.state, (*this.ptr).as_deref());
    }
}

impl<K, O, S: ?Sized, D> Observer for BTreeMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = BTreeMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Ord,
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

impl<K, O, S: ?Sized, D> BTreeMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BTreeMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero> + SerializeObserver,
    O::Head: Serialize + Sized + 'static,
    K: Serialize + Clone + Ord + Into<PathSegment> + 'static,
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
                    inner.remove(&key);
                    let value = (*self)
                        .untracked_ref()
                        .get(&key)
                        .expect("replaced key not found in observed map");
                    mutations.insert(key, Mutations::replace(value));
                }
            }
        }
        for (key, mut ob) in inner {
            let value = self
                .untracked_mut()
                .get_mut(&key)
                .expect("observer key not found in observed map");
            unsafe { O::relocate(&mut ob, value) }
            mutations.insert(key, unsafe { O::flush(&mut ob) });
        }
        mutations
    }
}

impl<K, O, S: ?Sized, D> SerializeObserver for BTreeMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BTreeMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero> + SerializeObserver,
    O::Head: Serialize + Sized + 'static,
    K: Serialize + Clone + Ord + Into<PathSegment> + 'static,
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
            diff.remove(key);
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

impl<K, O, S: ?Sized, D> BTreeMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BTreeMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Ord,
{
    /// See [`BTreeMap::get`].
    pub fn get<Q>(&self, key: &Q) -> Option<&O>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
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
}

impl<K, O, S: ?Sized, D> BTreeMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BTreeMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Ord,
{
    fn __force_all(&mut self) -> &mut BTreeMap<K, Box<O>> {
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

    /// See [`BTreeMap::get_mut`].
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut O>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let key_cloned = (*self.ptr).as_deref().get_key_value(key)?.0.clone();
        let value = (*self.ptr).as_deref_mut().get_mut(key)?;
        match self.state.inner.get_mut().entry(key_cloned) {
            Entry::Occupied(occupied) => {
                let ob = occupied.into_mut().as_mut();
                unsafe { O::relocate(ob, value) }
                Some(ob)
            }
            Entry::Vacant(vacant) => Some(vacant.insert(Box::new(O::observe(value)))),
        }
    }

    /// See [`BTreeMap::clear`].
    pub fn clear(&mut self) {
        self.state.inner.get_mut().clear();
        if (*self).untracked_ref().is_empty() {
            self.untracked_mut().clear()
        } else {
            self.tracked_mut().clear()
        }
    }

    /// See [`BTreeMap::insert`].
    pub fn insert(&mut self, key: K, value: O::Head) -> Option<O::Head> {
        if self.state.mutated {
            return self.tracked_mut().insert(key, value);
        }
        let key_cloned = key.clone();
        let old_value = (*self.ptr).as_deref_mut().insert(key_cloned, value);
        self.state.inner.get_mut().remove(&key);
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
        old_value
    }

    /// See [`BTreeMap::remove`].
    pub fn remove<Q>(&mut self, key: &Q) -> Option<O::Head>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if self.state.mutated {
            return self.tracked_mut().remove(key);
        }
        let (key, old_value) = (*self.ptr).as_deref_mut().remove_entry(key)?;
        self.state.mark_deleted(key);
        Some(old_value)
    }

    /// See [`BTreeMap::remove_entry`].
    pub fn remove_entry<Q>(&mut self, key: &Q) -> Option<(K, O::Head)>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if self.state.mutated {
            return self.tracked_mut().remove_entry(key);
        }
        let (key, old_value) = (*self.ptr).as_deref_mut().remove_entry(key)?;
        self.state.mark_deleted(key.clone());
        Some((key, old_value))
    }

    /// See [`BTreeMap::pop_first`].
    pub fn pop_first(&mut self) -> Option<(K, O::Head)> {
        if self.state.mutated {
            return self.tracked_mut().pop_first();
        }
        let (key, old_value) = (*self.ptr).as_deref_mut().pop_first()?;
        self.state.mark_deleted(key.clone());
        Some((key, old_value))
    }

    /// See [`BTreeMap::pop_last`].
    pub fn pop_last(&mut self) -> Option<(K, O::Head)> {
        if self.state.mutated {
            return self.tracked_mut().pop_last();
        }
        let (key, old_value) = (*self.ptr).as_deref_mut().pop_last()?;
        self.state.mark_deleted(key.clone());
        Some((key, old_value))
    }

    /// See [`BTreeMap::retain`].
    #[rustversion::since(1.91)]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &mut O::Head) -> bool,
    {
        self.extract_if(.., |k, v| !f(k, v)).for_each(drop);
    }

    /// See [`BTreeMap::append`].
    // TODO: this drains `other` into individual inserts, which is much slower than
    // `BTreeMap::append`. Consider a bulk-insert approach that updates `diff` in one pass.
    pub fn append(&mut self, other: &mut BTreeMap<K, O::Head>) {
        if self.state.mutated {
            return self.tracked_mut().append(other);
        }
        for (key, value) in std::mem::take(other) {
            self.insert(key, value);
        }
    }

    /// See [`BTreeMap::split_off`].
    pub fn split_off<Q>(&mut self, key: &Q) -> BTreeMap<K, O::Head>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if self.state.mutated {
            return self.tracked_mut().split_off(key);
        }
        let split = (*self.ptr).as_deref_mut().split_off(key);
        for key in split.keys().cloned() {
            self.state.mark_deleted(key);
        }
        split
    }

    /// See [`BTreeMap::extract_if`].
    #[rustversion::since(1.91)]
    pub fn extract_if<F, R>(&mut self, range: R, pred: F) -> ExtractIf<'_, K, O::Head, O, R, F>
    where
        R: RangeBounds<K>,
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

    /// See [`BTreeMap::iter_mut`].
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut O)> + '_ {
        self.__force_all().iter_mut().map(|(k, v)| (k, v.as_mut()))
    }

    /// See [`BTreeMap::values_mut`].
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut O> + '_ {
        self.__force_all().values_mut().map(|v| v.as_mut())
    }
}

impl<K, V, O, S: ?Sized, D> Debug for BTreeMapObserver<K, O, S, D>
where
    K: Clone + Ord,
    D: Unsigned,
    S: AsDeref<D, Target = BTreeMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    BTreeMap<K, V>: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BTreeMapObserver").field(&self.untracked_ref()).finish()
    }
}

impl<K, V, O, S: ?Sized, D> PartialEq<BTreeMap<K, V>> for BTreeMapObserver<K, O, S, D>
where
    K: Clone + Ord,
    D: Unsigned,
    S: AsDeref<D, Target = BTreeMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    BTreeMap<K, V>: PartialEq,
{
    fn eq(&self, other: &BTreeMap<K, V>) -> bool {
        self.untracked_ref().eq(other)
    }
}

impl<K1, K2, V1, V2, O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<BTreeMapObserver<K2, O2, S2, D2>>
    for BTreeMapObserver<K1, O1, S1, D1>
where
    K1: Clone + Ord,
    K2: Clone + Ord,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1, Target = BTreeMap<K1, V1>>,
    S2: AsDeref<D2, Target = BTreeMap<K2, V2>>,
    O1: Observer<InnerDepth = Zero, Head = V1>,
    O2: Observer<InnerDepth = Zero, Head = V2>,
    BTreeMap<K1, V1>: PartialEq<BTreeMap<K2, V2>>,
{
    fn eq(&self, other: &BTreeMapObserver<K2, O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<K, V, O, S: ?Sized, D> Eq for BTreeMapObserver<K, O, S, D>
where
    K: Clone + Ord,
    D: Unsigned,
    S: AsDeref<D, Target = BTreeMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    BTreeMap<K, V>: Eq,
{
}

impl<K, V, O, S: ?Sized, D> PartialOrd<BTreeMap<K, V>> for BTreeMapObserver<K, O, S, D>
where
    K: Clone + Ord,
    D: Unsigned,
    S: AsDeref<D, Target = BTreeMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    BTreeMap<K, V>: PartialOrd,
{
    fn partial_cmp(&self, other: &BTreeMap<K, V>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other)
    }
}

impl<K1, K2, V1, V2, O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<BTreeMapObserver<K2, O2, S2, D2>>
    for BTreeMapObserver<K1, O1, S1, D1>
where
    K1: Clone + Ord,
    K2: Clone + Ord,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1, Target = BTreeMap<K1, V1>>,
    S2: AsDeref<D2, Target = BTreeMap<K2, V2>>,
    O1: Observer<InnerDepth = Zero, Head = V1>,
    O2: Observer<InnerDepth = Zero, Head = V2>,
    BTreeMap<K1, V1>: PartialOrd<BTreeMap<K2, V2>>,
{
    fn partial_cmp(&self, other: &BTreeMapObserver<K2, O2, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<K, V, O, S: ?Sized, D> Ord for BTreeMapObserver<K, O, S, D>
where
    K: Clone + Ord,
    D: Unsigned,
    S: AsDeref<D, Target = BTreeMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    BTreeMap<K, V>: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

impl<'q, K, O, S: ?Sized, D, V, Q: ?Sized> Index<&'q Q> for BTreeMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BTreeMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    K: Borrow<Q> + Clone + Ord,
    Q: Ord,
{
    type Output = O;

    fn index(&self, index: &'q Q) -> &Self::Output {
        self.get(index).expect("no entry found for key")
    }
}

impl<'q, K, O, S: ?Sized, D, V, Q: ?Sized> IndexMut<&'q Q> for BTreeMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BTreeMap<K, V>>,
    O: Observer<InnerDepth = Zero, Head = V>,
    K: Borrow<Q> + Clone + Ord,
    Q: Ord,
{
    fn index_mut(&mut self, index: &'q Q) -> &mut Self::Output {
        self.get_mut(index).expect("no entry found for key")
    }
}

// TODO: this inserts elements one by one, which is much slower than `BTreeMap::extend`.
// Consider a bulk-insert approach that updates `diff` in one pass.
impl<K, O, S: ?Sized, D> Extend<(K, O::Head)> for BTreeMapObserver<K, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BTreeMap<K, O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
    K: Clone + Ord,
{
    fn extend<I: IntoIterator<Item = (K, O::Head)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<K: Clone + Ord, V: Observe> Observe for BTreeMap<K, V> {
    type Observer<'ob, S, D>
        = BTreeMapObserver<K, V::Observer<'ob, V, Zero>, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ref_observe! {
    impl [K, V] RefObserve for BTreeMap<K, V>;
}

impl<K, V> Snapshot for BTreeMap<K, V>
where
    K: Snapshot,
    K::Snapshot: Ord,
    V: Snapshot,
{
    type Snapshot = BTreeMap<K::Snapshot, V::Snapshot>;

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
    use std::collections::BTreeMap;

    use muon_test_utils::*;
    use serde_json::json;

    use crate::MutationKind;
    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn pointer_stability_across_inner_splits() {
        let mut map = BTreeMap::new();
        for i in 0..100 {
            map.insert(i, format!("value {i}"));
        }
        let ob = map.__observe();
        // Create observer for key 0
        assert_eq!(ob.get(&0).unwrap().untracked_ref(), "value 0");
        // Create many more observers, triggering node splits
        // Box<O> ensures previously created observers remain valid.
        for i in 1..100 {
            assert_eq!(ob.get(&i).unwrap().untracked_ref(), &format!("value {i}"));
        }
        // Key 0's observer is still valid thanks to Box pointer stability
        assert_eq!(ob.get(&0).unwrap().untracked_ref(), "value 0");
    }

    #[test]
    fn remove_nonexistent_key() {
        let mut map = BTreeMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        assert_eq!(ob.remove("nonexistent"), None);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn insert_then_remove() {
        let mut map = BTreeMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        assert_eq!(ob.insert("b", "y".to_string()), None);
        assert_eq!(ob.remove("b"), Some("y".to_string()));
        assert_eq!(ob.untracked_ref().len(), 1);
        assert_eq!(ob.untracked_ref().get("a"), Some(&"x".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn remove_then_insert() {
        let mut map = BTreeMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        assert_eq!(ob.remove("a"), Some("x".to_string()));
        assert_eq!(ob.insert("a", "y".to_string()), None);
        assert_eq!(ob.untracked_ref().get("a"), Some(&"y".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!("y"))));
    }

    #[test]
    fn get_mut_refresh_across_splits() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), "hello".to_string());
        let mut ob = map.__observe();
        // First get_mut: modify the value through the child observer
        ob.get_mut("a").unwrap().push_str(" world");
        assert_eq!(ob.untracked_ref().get("a").unwrap(), "hello world");
        // Insert many keys via untracked_mut to trigger node splits in the
        // observed BTreeMap without adding to diff.replaced
        for i in 1..100 {
            ob.untracked_mut().insert(i.to_string(), format!("value {i}"));
        }
        // Second get_mut: relocate updates the child observer's stale pointer
        ob.get_mut("a").unwrap().push_str("!");
        assert_eq!(ob.untracked_ref().get("a").unwrap(), "hello world!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(a, json!(" world!"))));
    }

    #[test]
    fn insert_then_get_mut() {
        let mut map = BTreeMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        ob.insert("b", "hello".to_string());
        ob.get_mut("b").unwrap().push_str(" world");
        assert_eq!(ob.untracked_ref().get("b"), Some(&"hello world".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(b, json!("hello world"))));
    }

    #[test]
    fn get_mut_then_insert() {
        let mut map = BTreeMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        ob.get_mut("a").unwrap().push_str(" world");
        ob.insert("a", "bye".to_string());
        assert_eq!(ob.untracked_ref().get("a"), Some(&"bye".to_string()));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!("bye"))));
    }

    #[test]
    fn remove_entry() {
        let mut map = BTreeMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
        let mut ob = map.__observe();
        assert_eq!(ob.remove_entry("a"), Some(("a", "x".to_string())));
        assert_eq!(ob.untracked_ref().len(), 1);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(a)));
    }

    #[test]
    fn pop_first_and_last() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        assert_eq!(ob.pop_first(), Some(("a", 1)));
        assert_eq!(ob.pop_last(), Some(("c", 3)));
        assert_eq!(ob.untracked_ref(), &BTreeMap::from([("b", 2)]));
        let Json(mutation) = ob.flush().unwrap();
        // Two deletions: "a" and "c"
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
    }

    #[test]
    fn retain() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        ob.retain(|_, v| *v % 2 != 0);
        assert_eq!(ob.untracked_ref(), &BTreeMap::from([("a", 1), ("c", 3)]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(b)));
    }

    #[test]
    fn append_from_other() {
        let mut map = BTreeMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        let mut other = BTreeMap::from([("b", "y".to_string()), ("c", "z".to_string())]);
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
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2), ("c", 3)]);
        let mut ob = map.__observe();
        let split = ob.split_off("b");
        assert_eq!(split, BTreeMap::from([("b", 2), ("c", 3)]));
        assert_eq!(ob.untracked_ref(), &BTreeMap::from([("a", 1)]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
    }

    #[test]
    fn extend() {
        let mut map = BTreeMap::from([("a", "x".to_string())]);
        let mut ob = map.__observe();
        ob.extend([("b", "y".to_string()), ("c", "z".to_string())]);
        assert_eq!(ob.untracked_ref().len(), 3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
    }

    #[test]
    fn extract_if() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2), ("c", 3), ("d", 4)]);
        let mut ob = map.__observe();
        let extracted: BTreeMap<_, _> = ob.extract_if(.., |_, v| *v % 2 == 0).collect();
        assert_eq!(extracted, BTreeMap::from([("b", 2), ("d", 4)]));
        assert_eq!(ob.untracked_ref(), &BTreeMap::from([("a", 1), ("c", 3)]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let mutation = mutation.unwrap();
        assert!(matches!(mutation.kind, MutationKind::Batch(_)));
    }

    #[test]
    fn extract_if_partial_drain() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2), ("c", 3), ("d", 4)]);
        let mut ob = map.__observe();
        // Only take the first matching element, then drop the iterator.
        let first = ob.extract_if(.., |_, v| *v % 2 == 0).next();
        assert_eq!(first, Some(("b", 2)));
        // "d" matched the predicate but was never yielded, so it must be retained.
        assert_eq!(ob.untracked_ref(), &BTreeMap::from([("a", 1), ("c", 3), ("d", 4)]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(delete!(b)));
    }

    #[test]
    fn extract_if_insert_then_extract() {
        let mut map = BTreeMap::from([("a", 1i32)]);
        let mut ob = map.__observe();
        ob.insert("b", 2);
        // extract "b" which was just inserted: net no-op
        let extracted: BTreeMap<_, _> = ob.extract_if(.., |k, _| *k == "b").collect();
        assert_eq!(extracted, BTreeMap::from([("b", 2)]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn iter_mut() {
        let mut map = BTreeMap::from([("a", "x".to_string()), ("b", "y".to_string())]);
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
            assert_eq!(batch[0].kind, MutationKind::Append(json!("!")));
            assert_eq!(batch[1].kind, MutationKind::Append(json!("!")));
        }
    }

    #[test]
    fn values_mut() {
        let mut map = BTreeMap::from([("a", "hello".to_string()), ("b", "world".to_string())]);
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
            assert_eq!(batch[0].kind, MutationKind::Append(json!("~")));
            assert_eq!(batch[1].kind, MutationKind::Append(json!("~")));
        }
    }

    #[test]
    fn insert_then_pop() {
        let mut map: BTreeMap<&str, i32> = BTreeMap::new();
        let mut ob = map.__observe();
        ob.insert("a", 1);
        ob.insert("b", 2);
        assert_eq!(ob.pop_first(), Some(("a", 1)));
        // "a" was inserted then popped: net no-op
        // "b" was inserted: Inserted
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(b, json!(2))));
    }

    #[test]
    fn flat_flush_no_change() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn flat_flush_deref_mut_only() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        *ob.tracked_mut() = BTreeMap::from([("a", 10), ("b", 20)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(a, json!(10)), replace!(b, json!(20))))
        );
    }

    // Inserted key, then deref_mut to a value without that key -> no Delete for the inserted key
    #[test]
    fn flat_flush_inserted_then_absent() {
        let mut map = BTreeMap::from([("a", 1i32)]);
        let mut ob = map.__observe();
        ob.insert("b", 2);
        *ob.tracked_mut() = BTreeMap::from([("a", 10)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!(10))));
    }

    // Inserted key, then deref_mut to a value with that key -> Replace for the key
    #[test]
    fn flat_flush_inserted_then_present() {
        let mut map = BTreeMap::from([("a", 1i32)]);
        let mut ob = map.__observe();
        ob.insert("b", 2);
        *ob.tracked_mut() = BTreeMap::from([("a", 10), ("b", 20)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(a, json!(10)), replace!(b, json!(20))))
        );
    }

    // Deleted key, then deref_mut to a value without that key -> Delete for the key
    #[test]
    fn flat_flush_deleted_then_absent() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.remove("b");
        *ob.tracked_mut() = BTreeMap::from([("a", 10)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, replace!(a, json!(10)), delete!(b))));
    }

    // Deleted key, then deref_mut to a value with that key -> Replace (not Delete)
    #[test]
    fn flat_flush_deleted_then_present() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.remove("b");
        *ob.tracked_mut() = BTreeMap::from([("a", 10), ("b", 20)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(a, json!(10)), replace!(b, json!(20))))
        );
    }

    // Replaced key, then deref_mut to a value without that key -> Delete for the key
    #[test]
    fn flat_flush_replaced_then_absent() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.insert("b", 99);
        *ob.tracked_mut() = BTreeMap::from([("a", 10)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, replace!(a, json!(10)), delete!(b))));
    }

    // Replaced key, then deref_mut to a value with that key -> Replace
    #[test]
    fn flat_flush_replaced_then_present() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.insert("b", 99);
        *ob.tracked_mut() = BTreeMap::from([("a", 10), ("b", 20)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(a, json!(10)), replace!(b, json!(20))))
        );
    }

    // Without deref_mut, flat_flush returns granular mutations with is_replace=false
    #[test]
    fn flat_flush_granular() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        ob.insert("a", 10);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(mutation, Some(replace!(a, json!(10))));
    }

    // deref_mut replaces with entirely new keys
    #[test]
    fn flat_flush_deref_mut_new_keys() {
        let mut map = BTreeMap::from([("a", 1i32), ("b", 2)]);
        let mut ob = map.__observe();
        *ob.tracked_mut() = BTreeMap::from([("c", 30)]);
        let Json(mutation) = ob.flat_flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(c, json!(30)), delete!(a), delete!(b)))
        );
    }
}
