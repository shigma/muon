//! Observer implementation for [`IndexSet<T>`].

use std::fmt::Debug;
use std::hash::Hash;
use std::iter::FusedIterator;
use std::ops::{Bound, Deref, DerefMut, RangeBounds};

use cfg_version::cfg_version;
use indexmap::{Equivalent, IndexSet, TryReserveError};
use serde::Serialize;

use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::{default_impl_ro_observe, delegate_methods};
use crate::helper::shallow::{ObserverState, SerializeObserverState, shallow_observer};
use crate::helper::{AsDerefMut, Invalidate, QuasiObserver, Unsigned};
use crate::observe::DefaultSpec;
use crate::{MutationKind, Mutations, Observe};

shallow_observer! {
    /// Observer implementation for [`IndexSet<T>`].
    struct IndexSetObserver<T>(IndexSet<T>, IndexSetObserverState<T>);
}

struct IndexSetObserverState<T> {
    truncate_len: usize,
    append_index: usize,
    phantom: std::marker::PhantomData<T>,
}

impl<T> Default for IndexSetObserverState<T> {
    fn default() -> Self {
        Self {
            truncate_len: 0,
            append_index: 0,
            phantom: std::marker::PhantomData,
        }
    }
}

impl<T> IndexSetObserverState<T> {
    fn mark_truncate(&mut self, index: usize) {
        if self.append_index <= index {
            return;
        }
        self.truncate_len += self.append_index - index;
        self.append_index = index;
    }
}

impl<T> Invalidate<IndexSet<T>> for IndexSetObserverState<T> {
    fn invalidate(&mut self, _set: &IndexSet<T>) {
        self.mark_truncate(0);
    }
}

impl<T> ObserverState<IndexSet<T>> for IndexSetObserverState<T> {
    fn observe(set: &IndexSet<T>) -> Self {
        Self {
            truncate_len: 0,
            append_index: set.len(),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<T: Serialize + Clone + 'static> SerializeObserverState<IndexSet<T>> for IndexSetObserverState<T> {
    fn flush(&mut self, set: &IndexSet<T>) -> Mutations {
        let append_index = std::mem::replace(&mut self.append_index, set.len());
        let truncate_len = std::mem::replace(&mut self.truncate_len, 0);

        if append_index == 0 && truncate_len > 0 {
            return Mutations::replace(set);
        }

        let mut mutations = Mutations::new();

        #[cfg(feature = "truncate")]
        if truncate_len > 0 {
            mutations.extend(crate::MutationKind::Truncate(truncate_len));
        }

        #[cfg(feature = "append")]
        {
            let appended: Vec<T> = set.iter().skip(append_index).cloned().collect();
            if !appended.is_empty() {
                mutations.extend(crate::MutationKind::Append(
                    Box::new(appended) as Box<dyn erased_serde::Serialize>
                ));
            }
        }

        mutations
    }
}

/// Guard that calls [`mark_truncate`](IndexSetObserverState::mark_truncate) on drop
/// with the current length of the inner set.
struct TruncateGuard<'a, T> {
    state: &'a mut IndexSetObserverState<T>,
    inner: &'a mut IndexSet<T>,
}

impl<T> Drop for TruncateGuard<'_, T> {
    fn drop(&mut self) {
        self.state.mark_truncate(self.inner.len());
    }
}

impl<T> Deref for TruncateGuard<'_, T> {
    type Target = IndexSet<T>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<T> DerefMut for TruncateGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl<'ob, T, S: ?Sized, D> IndexSetObserver<'ob, T, S, D>
where
    T: Eq + Hash,
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexSet<T>>,
{
    fn nonempty_mut(&mut self) -> &mut IndexSet<T> {
        if (*self).untracked_ref().is_empty() {
            self.untracked_mut()
        } else {
            self.tracked_mut()
        }
    }

    fn truncate_mut(&mut self) -> TruncateGuard<'_, T> {
        TruncateGuard {
            state: &mut self.state,
            inner: (*self.ptr).as_deref_mut(),
        }
    }

    delegate_methods! { nonempty_mut() as IndexSet =>
        pub fn clear(&mut self);
    }

    delegate_methods! { truncate_mut() as IndexSet =>
        pub fn truncate(&mut self, len: usize);
    }

    /// See [`IndexSet::drain`].
    pub fn drain<R>(&mut self, range: R) -> indexmap::set::Drain<'_, T>
    where
        R: RangeBounds<usize>,
    {
        let start_index = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        self.state.mark_truncate(start_index);
        self.untracked_mut().drain(range)
    }

    /// See [`IndexSet::extract_if`].
    #[cfg_version(indexmap = "2.10")]
    pub fn extract_if<F, R>(&mut self, range: R, mut pred: F) -> ExtractIf<'_, T, impl FnMut(&T) -> bool>
    where
        F: FnMut(&T) -> bool,
        R: RangeBounds<usize>,
    {
        let mut index = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let state = &mut self.state;
        let set = (*self.ptr).as_deref_mut();
        let inner = set.extract_if(range, move |v| {
            let is_extracted = pred(v);
            if is_extracted {
                state.mark_truncate(index);
            }
            index += 1;
            is_extracted
        });
        ExtractIf { inner }
    }

    /// See [`IndexSet::split_off`].
    pub fn split_off(&mut self, at: usize) -> IndexSet<T> {
        self.state.mark_truncate(at);
        self.untracked_mut().split_off(at)
    }

    delegate_methods! { untracked_mut() as IndexSet =>
        pub fn reserve(&mut self, additional: usize);
        pub fn reserve_exact(&mut self, additional: usize);
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
    }

    delegate_methods! { truncate_mut() as IndexSet =>
        pub fn pop(&mut self) -> Option<T>;
        #[cfg_version(indexmap = "2.12")]
        pub fn pop_if(&mut self, predicate: impl FnOnce(&T) -> bool) -> Option<T>;
    }

    /// See [`IndexSet::retain`].
    #[cfg_version(indexmap = "2.10")]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.extract_if(.., |v| !f(v)).for_each(drop);
    }

    delegate_methods! { nonempty_mut() as IndexSet =>
        pub fn sort(&mut self) where T: Ord;
        pub fn sort_by<F>(&mut self, cmp: F) where F: FnMut(&T, &T) -> std::cmp::Ordering;
        #[cfg_version(indexmap = "2.11")]
        pub fn sort_by_key<K, F>(&mut self, sort_key: F) where K: Ord, F: FnMut(&T) -> K;
        pub fn sort_unstable(&mut self) where T: Ord;
        pub fn sort_unstable_by<F>(&mut self, cmp: F) where F: FnMut(&T, &T) -> std::cmp::Ordering;
        #[cfg_version(indexmap = "2.11")]
        pub fn sort_unstable_by_key<K, F>(&mut self, sort_key: F) where K: Ord, F: FnMut(&T) -> K;
        pub fn sort_by_cached_key<K, F>(&mut self, sort_key: F) where K: Ord, F: FnMut(&T) -> K;
        pub fn reverse(&mut self);
    }

    /// See [`IndexSet::swap_remove_index`].
    pub fn swap_remove_index(&mut self, index: usize) -> Option<T> {
        self.state.mark_truncate(index);
        self.untracked_mut().swap_remove_index(index)
    }

    /// See [`IndexSet::shift_remove_index`].
    pub fn shift_remove_index(&mut self, index: usize) -> Option<T> {
        self.state.mark_truncate(index);
        self.untracked_mut().shift_remove_index(index)
    }

    delegate_methods! { nonempty_mut() as IndexSet =>
        // TODO
        pub fn move_index(&mut self, from: usize, to: usize);
        // TODO
        pub fn swap_indices(&mut self, a: usize, b: usize);
    }
}

impl<'ob, T, S: ?Sized, D> IndexSetObserver<'ob, T, S, D>
where
    T: Eq + Hash,
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexSet<T>>,
{
    /// See [`IndexSet::insert`].
    pub fn insert(&mut self, value: T) -> bool {
        let set = (*self.ptr).as_deref();
        if set.contains(&value) {
            return false;
        }
        self.untracked_mut().insert(value)
    }

    /// See [`IndexSet::insert_full`].
    pub fn insert_full(&mut self, value: T) -> (usize, bool) {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(&value) {
            return (index, false);
        }
        self.untracked_mut().insert_full(value)
    }

    /// See [`IndexSet::insert_sorted`].
    #[cfg_version(indexmap = "2.2.4")]
    pub fn insert_sorted(&mut self, value: T) -> (usize, bool)
    where
        T: Ord,
    {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(&value) {
            return (index, false);
        }
        let (index, inserted) = self.untracked_mut().insert_sorted(value);
        if inserted {
            self.state.mark_truncate(index);
        }
        (index, inserted)
    }

    // TODO
    /// See [`IndexSet::insert_sorted_by`].
    #[cfg_version(indexmap = "2.11")]
    pub fn insert_sorted_by<F>(&mut self, value: T, cmp: F) -> (usize, bool)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering,
    {
        // If the value exists, it gets moved; if not, it gets inserted. Either way the order
        // may change, so delegate to tracked_mut.
        self.tracked_mut().insert_sorted_by(value, cmp)
    }

    /// See [`IndexSet::insert_sorted_by_key`].
    #[cfg_version(indexmap = "2.11")]
    pub fn insert_sorted_by_key<K, F>(&mut self, value: T, sort_key: F) -> (usize, bool)
    where
        K: Ord,
        F: FnMut(&T) -> K,
    {
        self.tracked_mut().insert_sorted_by_key(value, sort_key)
    }

    /// See [`IndexSet::insert_before`].
    #[cfg_version(indexmap = "2.5")]
    pub fn insert_before(&mut self, index: usize, value: T) -> (usize, bool) {
        let set = (*self.ptr).as_deref();
        let existed = set.contains(&value);
        let old_index = if existed { set.get_index_of(&value) } else { None };
        let (result_index, inserted) = self.untracked_mut().insert_before(index, value);
        // Determine the first position that was disturbed.
        let disturbed = if let Some(old_idx) = old_index {
            // Existing element was moved: everything from min(old, new) is disturbed.
            old_idx.min(result_index)
        } else {
            // New element inserted at result_index, shifting everything after.
            result_index
        };
        self.state.mark_truncate(disturbed);
        (result_index, inserted)
    }

    /// See [`IndexSet::shift_insert`].
    #[cfg_version(indexmap = "2.2.3")]
    pub fn shift_insert(&mut self, index: usize, value: T) -> bool {
        let set = (*self.ptr).as_deref();
        let old_index = set.get_index_of(&value);
        let inserted = self.untracked_mut().shift_insert(index, value);
        let disturbed = if let Some(old_idx) = old_index {
            old_idx.min(index)
        } else {
            index
        };
        self.state.mark_truncate(disturbed);
        inserted
    }

    /// See [`IndexSet::replace`].
    pub fn replace(&mut self, value: T) -> Option<T> {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(&value) {
            self.state.mark_truncate(index);
        }
        self.untracked_mut().replace(value)
    }

    /// See [`IndexSet::replace_full`].
    #[cfg_version(indexmap = "2.11")]
    pub fn replace_full(&mut self, value: T) -> (usize, Option<T>) {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(&value) {
            self.state.mark_truncate(index);
        }
        self.untracked_mut().replace_full(value)
    }

    /// See [`IndexSet::replace_index`].
    #[cfg_version(indexmap = "2.11")]
    pub fn replace_index(&mut self, index: usize, value: T) -> Result<T, (usize, T)> {
        self.state.mark_truncate(index);
        self.untracked_mut().replace_index(index, value)
    }

    /// See [`IndexSet::splice`].
    ///
    /// Note: the returned [`Splice`](indexmap::set::Splice) iterator may silently skip
    /// duplicate values from `replace_with`. Any element in the replaced range causes
    /// a truncation from the start of the range.
    #[cfg_version(indexmap = "2.2")]
    pub fn splice<R, I>(&mut self, range: R, replace_with: I) -> Vec<T>
    where
        R: RangeBounds<usize>,
        I: IntoIterator<Item = T>,
    {
        let start_index = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        self.state.mark_truncate(start_index);
        self.untracked_mut().splice(range, replace_with).collect()
    }

    /// See [`IndexSet::append`].
    #[cfg_version(indexmap = "2.4")]
    pub fn append(&mut self, other: &mut IndexSet<T>) {
        // IndexSet::append keeps existing elements in place and appends only new ones.
        // However, if `other` contains duplicates of elements already in our set,
        // those are silently skipped. So this is safe as a plain untracked append.
        self.untracked_mut().append(other);
    }
}

impl<'ob, T, S: ?Sized, D> IndexSetObserver<'ob, T, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexSet<T>>,
{
    /// See [`IndexSet::swap_remove`].
    pub fn swap_remove<Q>(&mut self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<T>,
    {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(value) {
            self.state.mark_truncate(index);
            self.untracked_mut().swap_remove(value);
            return true;
        }
        false
    }

    /// See [`IndexSet::shift_remove`].
    pub fn shift_remove<Q>(&mut self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<T>,
    {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(value) {
            self.state.mark_truncate(index);
            self.untracked_mut().shift_remove(value);
            return true;
        }
        false
    }

    /// See [`IndexSet::swap_take`].
    pub fn swap_take<Q>(&mut self, value: &Q) -> Option<T>
    where
        Q: ?Sized + Hash + Equivalent<T>,
    {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(value) {
            self.state.mark_truncate(index);
        }
        self.untracked_mut().swap_take(value)
    }

    /// See [`IndexSet::shift_take`].
    pub fn shift_take<Q>(&mut self, value: &Q) -> Option<T>
    where
        Q: ?Sized + Hash + Equivalent<T>,
    {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(value) {
            self.state.mark_truncate(index);
        }
        self.untracked_mut().shift_take(value)
    }

    /// See [`IndexSet::swap_remove_full`].
    pub fn swap_remove_full<Q>(&mut self, value: &Q) -> Option<(usize, T)>
    where
        Q: ?Sized + Hash + Equivalent<T>,
    {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(value) {
            self.state.mark_truncate(index);
        }
        self.untracked_mut().swap_remove_full(value)
    }

    /// See [`IndexSet::shift_remove_full`].
    pub fn shift_remove_full<Q>(&mut self, value: &Q) -> Option<(usize, T)>
    where
        Q: ?Sized + Hash + Equivalent<T>,
    {
        let set = (*self.ptr).as_deref();
        if let Some(index) = set.get_index_of(value) {
            self.state.mark_truncate(index);
        }
        self.untracked_mut().shift_remove_full(value)
    }
}

impl<'ob, T, S: ?Sized, D, U> Extend<U> for IndexSetObserver<'ob, T, S, D>
where
    T: Eq + Hash,
    D: Unsigned,
    S: AsDerefMut<D, Target = IndexSet<T>>,
    IndexSet<T>: Extend<U>,
{
    fn extend<I: IntoIterator<Item = U>>(&mut self, iter: I) {
        // IndexSet::extend only appends truly new elements at the end,
        // existing elements are not moved. Safe as untracked.
        self.untracked_mut().extend(iter);
    }
}

/// Iterator produced by [`IndexSetObserver::extract_if`].
#[cfg_version(indexmap = "2.10")]
pub struct ExtractIf<'a, T, F>
where
    F: FnMut(&T) -> bool,
{
    inner: indexmap::set::ExtractIf<'a, T, F>,
}

#[cfg_version(indexmap = "2.10")]
impl<T, F> Iterator for ExtractIf<'_, T, F>
where
    F: FnMut(&T) -> bool,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[cfg_version(indexmap = "2.10")]
impl<T, F> FusedIterator for ExtractIf<'_, T, F> where F: FnMut(&T) -> bool {}

#[cfg_version(indexmap = "2.10")]
impl<T, F> Debug for ExtractIf<'_, T, F>
where
    F: FnMut(&T) -> bool,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractIf").finish_non_exhaustive()
    }
}

impl<T: Clone + Eq + Hash> Observe for IndexSet<T> {
    type Observer<'ob, S, D>
        = IndexSetObserver<'ob, T, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ro_observe! {
    impl [T] RoObserve for IndexSet<T>;
}

impl<T: Serialize + Clone + Eq + Hash> Snapshot for IndexSet<T> {
    type Snapshot = Box<[T]>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.iter().cloned().collect()
    }
}

struct AppendTail<'a, T> {
    set: &'a IndexSet<T>,
    skip: usize,
}

impl<T: Serialize> Serialize for AppendTail<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let count = self.set.len() - self.skip;
        let mut seq = serializer.serialize_seq(Some(count))?;
        for item in self.set.iter().skip(self.skip) {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

impl<T: Serialize + Clone + Eq + Hash> SerializeSnapshot for IndexSet<T> {
    fn flush(&self, snapshot: Self::Snapshot) -> Mutations {
        let prefix_len = self.iter().zip(snapshot.iter()).take_while(|(a, b)| *a == *b).count();
        if prefix_len == self.len() && prefix_len == snapshot.len() {
            return Mutations::new();
        }
        if prefix_len == 0 {
            return Mutations::replace(self);
        }
        let mut mutations = Mutations::new();
        if snapshot.len() > prefix_len {
            #[cfg(feature = "truncate")]
            mutations.extend(MutationKind::Truncate(snapshot.len() - prefix_len));
            #[cfg(not(feature = "truncate"))]
            return Mutations::replace(self);
        }
        if self.len() > prefix_len {
            #[cfg(feature = "append")]
            mutations.extend(Mutations::append_owned(AppendTail { set: self, skip: prefix_len }));
            #[cfg(not(feature = "append"))]
            return Mutations::replace(self);
        }
        mutations
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexSet;
    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_change() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn insert_append() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.insert(4);
        ob.insert(5);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([4, 5]))));
    }

    #[test]
    fn insert_duplicate_no_mutation() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert!(!ob.insert(2));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn insert_sorted_truncate_append() {
        let mut set = IndexSet::from([1, 3, 5]);
        let mut ob = set.__observe();
        assert_eq!(ob.insert_sorted(2), (1, true));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 2), append!(_, json!([2, 3, 5])))));
    }

    #[test]
    fn swap_remove_last_as_truncate() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.swap_remove(&3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 1)));
    }

    #[test]
    fn swap_remove_middle() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        // swap_remove(2) swaps 2 with 5, then pops -> set becomes [1, 5, 3, 4]
        ob.swap_remove(&2);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([5, 3, 4])))));
    }

    #[test]
    fn shift_remove_middle() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        ob.shift_remove(&2);
        // set becomes [1, 3, 4, 5]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([3, 4, 5])))));
    }

    #[test]
    fn remove_nonexistent_no_mutation() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert!(!ob.swap_remove(&99));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn clear_non_empty() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([]))));
    }

    #[test]
    fn clear_empty() {
        let mut set: IndexSet<i32> = IndexSet::new();
        let mut ob = set.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn deref_mut_triggers_replace() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        *ob.tracked_mut() = IndexSet::from([4, 5]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([4, 5]))));
    }

    #[test]
    fn double_flush() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.insert(4);
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn pop_as_truncate() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert_eq!(ob.pop(), Some(3));
        assert_eq!(ob.pop(), Some(2));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 2)));
    }

    #[test]
    fn truncate() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        ob.truncate(2);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 3)));
    }

    #[test]
    fn retain_noop() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        ob.retain(|v| *v < 10);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn retain_truncate() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        ob.retain(|v| *v % 2 == 1);
        // set becomes [1, 3, 5]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([3, 5])))));
    }

    #[test]
    fn extract_if_noop() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        let extracted: Vec<_> = ob.extract_if(.., |_| false).collect();
        assert!(extracted.is_empty());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn extract_if_partial() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        let extracted: Vec<_> = ob.extract_if(.., |v| *v % 2 == 0).collect();
        assert_eq!(extracted, vec![2, 4]);
        // set becomes [1, 3, 5]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([3, 5])))));
    }

    #[test]
    fn extract_if_drop_early() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        let mut iter = ob.extract_if(.., |v| *v % 2 == 0);
        assert_eq!(iter.next(), Some(2));
        drop(iter);
        // Only 2 was extracted, set becomes [1, 3, 4, 5]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([3, 4, 5])))));
    }

    #[test]
    fn drain_range() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        let drained: Vec<_> = ob.drain(1..3).collect();
        assert_eq!(drained, vec![2, 3]);
        // set becomes [1, 4, 5]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([4, 5])))));
    }

    #[test]
    fn drain_all() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        let _: Vec<_> = ob.drain(..).collect();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([]))));
    }

    #[test]
    fn split_off() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        let split = ob.split_off(2);
        assert_eq!(split, IndexSet::from([3, 4, 5]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 3)));
    }

    #[test]
    fn replace() {
        let mut set = IndexSet::from(["a".to_string(), "b".to_string(), "c".to_string()]);
        let mut ob = set.__observe();
        ob.replace("b".to_string());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 2), append!(_, json!(["b", "c"]))))
        );
    }

    #[test]
    fn swap_take() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        assert_eq!(ob.swap_take(&3), Some(3));
        // 3 was at index 2, swap with last (5) -> set becomes [1, 2, 5, 4]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 3), append!(_, json!([5, 4])))));
    }

    #[test]
    fn shift_take() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        assert_eq!(ob.shift_take(&3), Some(3));
        // set becomes [1, 2, 4, 5]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 3), append!(_, json!([4, 5])))));
    }

    #[test]
    fn extend_new_elements() {
        let mut set = IndexSet::from([1, 2]);
        let mut ob = set.__observe();
        ob.extend([3, 4, 5]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([3, 4, 5]))));
    }

    #[test]
    fn extend_duplicates_no_mutation() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.extend([1, 2, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn append_from_other() {
        let mut set = IndexSet::from([1, 2]);
        let mut ob = set.__observe();
        let mut other = IndexSet::from([2, 3, 4]);
        ob.append(&mut other);
        assert!(other.is_empty());
        // 2 already exists so only 3, 4 are appended
        assert_eq!(*ob.untracked_ref(), IndexSet::from([1, 2, 3, 4]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([3, 4]))));
    }

    #[test]
    fn insert_before_new() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.insert_before(1, 10);
        // set becomes [1, 10, 2, 3]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 2), append!(_, json!([10, 2, 3]))))
        );
    }

    #[test]
    fn swap_remove_index() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        assert_eq!(ob.swap_remove_index(1), Some(2));
        // set becomes [1, 5, 3, 4]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([5, 3, 4])))));
    }

    #[test]
    fn shift_remove_index() {
        let mut set = IndexSet::from([1, 2, 3, 4, 5]);
        let mut ob = set.__observe();
        assert_eq!(ob.shift_remove_index(1), Some(2));
        // set becomes [1, 3, 4, 5]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([3, 4, 5])))));
    }

    #[test]
    fn sort_triggers_replace() {
        let mut set = IndexSet::from([3, 1, 2]);
        let mut ob = set.__observe();
        ob.sort();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1, 2, 3]))));
    }

    #[test]
    fn reverse_triggers_replace() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.reverse();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([3, 2, 1]))));
    }

    #[test]
    fn replace_index() {
        let mut set = IndexSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert_eq!(ob.replace_index(1, 20), Ok(2));
        // set becomes [1, 20, 3]
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 2), append!(_, json!([20, 3])))));
    }
}

#[cfg(test)]
mod snapshot_tests {
    use indexmap::IndexSet;
    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::{Adapter, Json};
    use crate::general::{SerializeSnapshot, Snapshot};

    #[test]
    fn no_change() {
        let set = IndexSet::from([1, 2, 3]);
        let snapshot = set.to_snapshot();
        let mutations = set.flush(snapshot);
        assert!(mutations.is_empty());
    }

    #[test]
    fn append_elements() {
        let set = IndexSet::from([1, 2, 3, 4, 5]);
        let snapshot = IndexSet::from([1, 2, 3]).to_snapshot();
        let Json(mutation) = Json::from_mutations(set.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(append!(_, json!([4, 5]))));
    }

    #[test]
    fn truncate_elements() {
        let set = IndexSet::from([1, 2]);
        let snapshot = IndexSet::from([1, 2, 3, 4]).to_snapshot();
        let Json(mutation) = Json::from_mutations(set.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(truncate!(_, 2)));
    }

    #[test]
    fn diverge_in_middle() {
        let set = IndexSet::from([1, 2, 99, 100]);
        let snapshot = IndexSet::from([1, 2, 3, 4, 5]).to_snapshot();
        let Json(mutation) = Json::from_mutations(set.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 3), append!(_, json!([99, 100])))));
    }

    #[test]
    fn all_different() {
        let set = IndexSet::from([4, 5, 6]);
        let snapshot = IndexSet::from([1, 2, 3]).to_snapshot();
        let Json(mutation) = Json::from_mutations(set.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([4, 5, 6]))));
    }

    #[test]
    fn empty_to_nonempty() {
        let set = IndexSet::from([1, 2, 3]);
        let snapshot = IndexSet::<i32>::new().to_snapshot();
        let Json(mutation) = Json::from_mutations(set.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1, 2, 3]))));
    }

    #[test]
    fn nonempty_to_empty() {
        let set = IndexSet::<i32>::new();
        let snapshot = IndexSet::from([1, 2, 3]).to_snapshot();
        let Json(mutation) = Json::from_mutations(set.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([]))));
    }
}
