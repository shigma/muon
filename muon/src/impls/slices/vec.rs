//! Observer implementation for [`Vec<T>`].

use std::cell::UnsafeCell;
use std::collections::TryReserveError;
use std::fmt::Debug;
use std::io::Write;
use std::mem::MaybeUninit;
use std::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};
use std::vec::{Drain, ExtractIf, Splice};

use serde::Serialize;

use crate::general::Snapshot;
use crate::helper::macros::{default_impl_ref_observe, delegate_methods};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::impls::slice::{LazyVec, SliceObserver, SliceObserverState, SliceSerializeObserverState};
use crate::impls::slices::helper::SliceIndexImpl;
use crate::observe::{DefaultSpec, Observer, SerializeObserver};
use crate::{MutationKind, Mutations, Observe, PathSegment};

/// Observer state for dynamically-sized slices ([`Vec<T>`], [`Box<[T]>`](Box)), tracking
/// [`Append`](MutationKind::Append) and [`Truncate`](MutationKind::Truncate) boundaries.
///
/// The `append_index` divides the observed slice into two regions: elements before it are
/// "existing" (may have individual observer state), and elements from `append_index` onward are
/// "appended" (new since the last flush).
///
/// ## Replace Semantics
///
/// During [`flush`](SliceSerializeObserverState::flush), if all existing elements' inner observers
/// report [`Replace`](MutationKind::Replace) and there was at least some tracked content
/// (`append_index > 0` or `truncate_len > 0`), the granular mutations are collapsed into a single
/// whole-slice [`Replace`](MutationKind::Replace).
pub struct VecObserverState<O> {
    /// Number of elements truncated from the end since the last flush.
    truncate_len: usize,
    /// Starting index of appended elements. Elements before this index are "existing" and have
    /// their inner observers flushed individually.
    append_index: usize,
    /// Lazily-initialized element observer storage.
    ///
    /// Unlike map observers which use [`Box<O>`] for pointer stability across rehashing /
    /// node-splits, we store observers inline in a [`Vec<MaybeUninit<O>>`]. The `data` vector
    /// is always sized to match the observed slice length (resized during [`relocate`]),
    /// and `initialized` tracks which slots contain valid observers.
    ///
    /// Since the slice length can only change through `&mut self` operations
    /// ([`push`](Vec::push), [`pop`](Vec::pop), etc.), it stays constant for the
    /// entire duration of any `&self` borrow. Therefore, the first
    /// [`relocate`](SliceObserverState::relocate) call sizes `data` to its final length,
    /// and subsequent calls within the same `&self` borrow lifetime never trigger reallocation,
    /// keeping all previously returned references valid.
    inner: UnsafeCell<LazyVec<O>>,
}

impl<O> VecObserverState<O> {
    fn mark_truncate(&mut self, index: usize) {
        if self.append_index <= index {
            return;
        }
        self.truncate_len += self.append_index - index;
        self.append_index = index;
        self.inner.get_mut().truncate(index);
    }

    fn mark_replace(&mut self) {
        self.mark_truncate(0);
    }
}

impl<O> Invalidate<[O::Head]> for VecObserverState<O>
where
    O: Observer<InnerDepth = Zero, Head: Sized>,
{
    fn invalidate(&mut self, _: &[O::Head]) {
        self.mark_replace();
    }
}

impl<O> SliceObserverState for VecObserverState<O>
where
    O: Observer<InnerDepth = Zero, Head: Sized>,
{
    type Target = [O::Head];
    type Item = O;

    fn observe(slice: &mut Self::Target) -> Self {
        Self {
            truncate_len: 0,
            append_index: slice.as_ref().len(),
            inner: UnsafeCell::new(LazyVec::new()),
        }
    }

    fn get<I: SliceIndexImpl>(&self, index: I, slice: &mut Self::Target) -> Option<&I::Output<O>> {
        let range = index.to_range(slice.as_ref().len());
        let inner = unsafe { &mut *self.inner.get() };
        inner.relocate(range, slice);
        let output = index.get(&inner.data)?;
        Some(unsafe { std::mem::transmute_copy(&output) })
    }

    fn get_mut<I: SliceIndexImpl>(&mut self, index: I, slice: &mut Self::Target) -> Option<&mut I::Output<O>> {
        let range = index.to_range(slice.as_ref().len());
        let inner = self.inner.get_mut();
        inner.relocate(range, slice);
        let output = index.get_mut(&mut inner.data)?;
        Some(unsafe { std::mem::transmute_copy(&output) })
    }
}

impl<O, S, D> SliceSerializeObserverState<S, D> for VecObserverState<O>
where
    D: Unsigned,
    S: AsDeref<D, Target = [O::Head]> + ?Sized,
    O: Observer<InnerDepth = Zero> + SerializeObserver,
    O::Head: Serialize + Sized + 'static,
{
    type Target = [O::Head];

    fn flush(&mut self, ptr: &mut Pointer<S>) -> Mutations {
        let slice = (**ptr).as_deref();
        let append_index = core::mem::replace(&mut self.append_index, slice.len());
        let truncate_len = core::mem::replace(&mut self.truncate_len, 0);

        if cfg!(not(feature = "truncate")) && truncate_len > 0
            || cfg!(not(feature = "append")) && slice.len() > append_index
        {
            self.inner.get_mut().truncate(0);
            return Mutations::replace(slice);
        }

        // Phase 1: Relocate initialized observers.
        let inner = self.inner.get_mut();
        let has_gaps = inner.initialized.gaps(&(0..append_index)).next().is_some();
        for range in inner.initialized.overlapping(&(0..append_index)) {
            let end = range.end.min(append_index);
            for (i, head) in slice.iter().enumerate().skip(range.start).take(end - range.start) {
                unsafe { Observer::relocate(inner.data[i].assume_init_mut(), head as *const O::Head as *mut O::Head) };
            }
        }

        // Phase 2: Build Truncate/Append (safe to create SerializeRef now).
        let mut mutations = Mutations::new();
        if truncate_len > 0 {
            mutations.extend(MutationKind::Truncate(truncate_len));
        }
        if slice.len() > append_index {
            mutations.extend(Mutations::append(&slice[append_index..]));
        }

        // Phase 3: Flush initialized observers in reverse, appending child mutations.
        let mut is_replace = !has_gaps;
        for range in inner.initialized.overlapping(&(0..append_index)).rev() {
            let end = range.end.min(append_index);
            for i in (range.start..end).rev() {
                let mutations_i = unsafe { SerializeObserver::flush(inner.data[i].assume_init_mut()) };
                is_replace &= mutations_i.is_replace();
                mutations.insert(PathSegment::Negative(slice.len() - i), mutations_i);
            }
        }
        if is_replace && !mutations.is_empty() {
            return Mutations::replace(slice);
        };
        mutations
    }
}

/// Observer implementation for [`Vec<T>`].
pub struct VecObserver<O, S: ?Sized, D = Zero> {
    inner: SliceObserver<VecObserverState<O>, S, Succ<D>>,
}

impl<O, S: ?Sized, D> Deref for VecObserver<O, S, D> {
    type Target = SliceObserver<VecObserverState<O>, S, Succ<D>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<O, S: ?Sized, D> DerefMut for VecObserver<O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<O, S: ?Sized, D> QuasiObserver for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Vec<O::Head>>,
    O: Observer<InnerDepth = Zero, Head: Sized>,
{
    type Head = S;
    type OuterDepth = Succ<Succ<Zero>>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        // SliceObserver::invalidate(&mut this.inner);
        Invalidate::invalidate(&mut this.inner.state, (*this.inner.ptr).as_deref());
    }
}

impl<O, S: ?Sized, D, T> Observer for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Vec<T>>,
    O: Observer<InnerDepth = Zero, Head = T>,
{
    fn observe(head: &mut Self::Head) -> Self {
        Self {
            inner: Observer::observe(head),
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe { Observer::relocate(&mut this.inner, head) }
    }
}

impl<O, S: ?Sized, D, T> SerializeObserver for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Vec<T>>,
    O: Observer<InnerDepth = Zero, Head = T> + SerializeObserver,
    T: Serialize + 'static,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        unsafe { SliceObserver::flush(&mut this.inner) }
    }
}

struct TruncateGuard<'a, O, T> {
    state: &'a mut VecObserverState<O>,
    inner: &'a mut Vec<T>,
}

impl<O, T> Drop for TruncateGuard<'_, O, T> {
    fn drop(&mut self) {
        self.state.mark_truncate(self.inner.len());
    }
}

impl<O, T> Deref for TruncateGuard<'_, O, T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<O, T> DerefMut for TruncateGuard<'_, O, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl<O, S: ?Sized, D, T> VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Vec<T>>,
    O: Observer<InnerDepth = Zero, Head = T>,
{
    fn nonempty_mut(&mut self) -> &mut Vec<T> {
        if (*self).untracked_ref().is_empty() {
            self.untracked_mut()
        } else {
            self.tracked_mut()
        }
    }

    fn truncate_mut(&mut self) -> TruncateGuard<'_, O, T> {
        TruncateGuard {
            state: &mut self.inner.state,
            inner: (*self.inner.ptr).as_deref_mut(),
        }
    }

    delegate_methods! { untracked_mut() as Vec =>
        pub fn push(&mut self, value: T);
    }

    /// See [`Vec::push_mut`].
    #[rustversion::since(1.95)]
    pub fn push_mut(&mut self, value: T) -> &mut O {
        self.untracked_mut().push(value);
        self.force_mut().last_mut().unwrap()
    }

    delegate_methods! { untracked_mut() as Vec =>
        pub fn reserve(&mut self, additional: usize);
        pub fn reserve_exact(&mut self, additional: usize);
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
    }

    delegate_methods! { truncate_mut() as Vec =>
        pub fn truncate(&mut self, len: usize);
    }

    /// See [`Vec::as_mut_slice`].
    pub fn as_mut_slice(&mut self) -> &mut [O] {
        self.force_mut()
    }

    delegate_methods! { tracked_mut() as Vec =>
        pub fn as_mut_ptr(&mut self) -> *mut T;
    }

    delegate_methods! { truncate_mut() as Vec =>
        pub unsafe fn set_len(&mut self, new_len: usize);
    }

    /// See [`Vec::swap_remove`].
    pub fn swap_remove(&mut self, index: usize) -> T {
        let value = self.untracked_mut().swap_remove(index);
        self.state.mark_truncate(index);
        value
    }

    /// See [`Vec::insert`].
    pub fn insert(&mut self, index: usize, element: T) {
        self.untracked_mut().insert(index, element);
        self.state.mark_truncate(index);
    }

    /// See [`Vec::insert_mut`].
    #[rustversion::since(1.95)]
    pub fn insert_mut(&mut self, index: usize, element: T) -> &mut O {
        self.state.mark_truncate(index);
        self.untracked_mut().insert(index, element);
        // Drop stale observers at and beyond `index` — those slots now hold shifted elements.
        self.state.inner.get_mut().truncate(index);
        &mut self.force_mut()[index]
    }

    /// See [`Vec::remove`].
    pub fn remove(&mut self, index: usize) -> T {
        let value = self.untracked_mut().remove(index);
        self.state.mark_truncate(index);
        value
    }

    /// See [`Vec::retain`].
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.retain_mut(|v| f(v));
    }

    /// See [`Vec::retain_mut`].
    pub fn retain_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let mut index = 0;
        (*self.inner.ptr).as_deref_mut().retain_mut(|v| {
            let is_retained = f(v);
            if !is_retained {
                self.inner.state.mark_truncate(index);
            }
            index += 1;
            is_retained
        });
    }

    delegate_methods! { nonempty_mut() as Vec =>
        pub fn dedup_by_key<F, K>(&mut self, key: F) where F: FnMut(&mut T) -> K, K: PartialEq;
        pub fn dedup_by<F>(&mut self, same_bucket: F) where F: FnMut(&mut T, &mut T) -> bool;
    }

    delegate_methods! { truncate_mut() as Vec =>
        pub fn pop(&mut self) -> Option<T>;
        pub fn pop_if(&mut self, predicate: impl FnOnce(&mut T) -> bool) -> Option<T>;
    }

    delegate_methods! { untracked_mut() as Vec =>
        pub fn append(&mut self, other: &mut Vec<T>);
    }

    /// See [`Vec::drain`].
    pub fn drain<R>(&mut self, range: R) -> Drain<'_, T>
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

    delegate_methods! { nonempty_mut() as Vec =>
        pub fn clear(&mut self);
    }

    delegate_methods! { truncate_mut() as Vec =>
        pub fn split_off(&mut self, at: usize) -> Vec<T>;
        pub fn resize_with<F>(&mut self, new_len: usize, f: F) where F: FnMut() -> T;
    }

    delegate_methods! { untracked_mut() as Vec =>
        pub fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<T>];
    }

    /// See [`Vec::splice`].
    pub fn splice<R, I>(&mut self, range: R, replace_with: I) -> Splice<'_, I::IntoIter>
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
        self.untracked_mut().splice(range, replace_with)
    }

    /// See [`Vec::extract_if`].
    pub fn extract_if<F, R>(&mut self, range: R, mut filter: F) -> ExtractIf<'_, T, impl FnMut(&mut T) -> bool>
    where
        F: FnMut(&mut T) -> bool,
        R: RangeBounds<usize>,
    {
        let mut index = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let state = &mut self.inner.state;
        let vec = (*self.inner.ptr).as_deref_mut();
        vec.extract_if(range, move |v| {
            let is_extracted = filter(v);
            if is_extracted {
                state.mark_truncate(index);
            }
            index += 1;
            is_extracted
        })
    }
}

impl<O, S: ?Sized, D, T> VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Vec<T>>,
    O: Observer<InnerDepth = Zero, Head = T>,
    T: Clone,
{
    delegate_methods! { truncate_mut() as Vec =>
        pub fn resize(&mut self, new_len: usize, value: T);
    }

    delegate_methods! { untracked_mut() as Vec =>
        pub fn extend_from_slice(&mut self, other: &[T]);
        pub fn extend_from_within<R>(&mut self, src: R) where R: RangeBounds<usize>;
    }
}

impl<O, S: ?Sized, D, T> VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Vec<T>>,
    O: Observer<InnerDepth = Zero, Head = T>,
    T: PartialEq,
{
    delegate_methods! { nonempty_mut() as Vec =>
        pub fn dedup(&mut self);
    }
}

impl<O, S: ?Sized, D, T, U> Extend<U> for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Vec<T>>,
    O: Observer<InnerDepth = Zero, Head = T>,
    Vec<T>: Extend<U>,
{
    fn extend<I: IntoIterator<Item = U>>(&mut self, other: I) {
        self.untracked_mut().extend(other);
    }
}

impl<O, S: ?Sized, D> Write for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Vec<u8>>,
    O: Observer<InnerDepth = Zero, Head = u8>,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.untracked_mut().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<O, S: ?Sized, D> Debug for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Vec<O::Head>>,
    O: Observer<InnerDepth = Zero, Head: Sized + Debug>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("VecObserver").field(&self.untracked_ref()).finish()
    }
}

macro_rules! generic_impl_partial_eq {
    ($(impl $([$($gen:tt)*])? PartialEq<$ty:ty> for Vec<_>);* $(;)?) => {
        $(
            impl<$($($gen)*,)? O, S: ?Sized, D> PartialEq<$ty> for VecObserver<O, S, D>
            where
                D: Unsigned,
                S: AsDeref<D, Target = Vec<O::Head>>,
                O: Observer<InnerDepth = Zero, Head: Sized>,
                Vec<O::Head>: PartialEq<$ty>,
            {
                fn eq(&self, other: &$ty) -> bool {
                    self.untracked_ref().eq(other)
                }
            }
        )*
    };
}

generic_impl_partial_eq! {
    impl [U] PartialEq<Vec<U>> for Vec<_>;
    impl [U] PartialEq<[U]> for Vec<_>;
    impl ['a, U] PartialEq<&'a U> for Vec<_>;
    impl ['a, U] PartialEq<&'a mut U> for Vec<_>;
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<VecObserver<O2, S2, D2>> for VecObserver<O1, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    O1: Observer<InnerDepth = Zero, Head: Sized>,
    O2: Observer<InnerDepth = Zero, Head: Sized>,
    S1: AsDeref<D1, Target = Vec<O1::Head>>,
    S2: AsDeref<D2, Target = Vec<O2::Head>>,
    Vec<O1::Head>: PartialEq<Vec<O2::Head>>,
{
    fn eq(&self, other: &VecObserver<O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Eq for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Vec<O::Head>>,
    O: Observer<InnerDepth = Zero, Head: Sized + Eq>,
{
}

impl<O, S: ?Sized, D, U> PartialOrd<Vec<U>> for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Vec<O::Head>>,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    Vec<O::Head>: PartialOrd<Vec<U>>,
{
    fn partial_cmp(&self, other: &Vec<U>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other)
    }
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<VecObserver<O2, S2, D2>> for VecObserver<O1, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    O1: Observer<InnerDepth = Zero, Head: Sized>,
    O2: Observer<InnerDepth = Zero, Head: Sized>,
    S1: AsDeref<D1, Target = Vec<O1::Head>>,
    S2: AsDeref<D2, Target = Vec<O2::Head>>,
    Vec<O1::Head>: PartialOrd<Vec<O2::Head>>,
{
    fn partial_cmp(&self, other: &VecObserver<O2, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Ord for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Vec<O::Head>>,
    O: Observer<InnerDepth = Zero, Head: Sized + Ord>,
{
    fn cmp(&self, other: &VecObserver<O, S, D>) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D, T, I> Index<I> for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Vec<T>>,
    O: Observer<InnerDepth = Zero, Head = T>,
    I: SliceIndexImpl,
{
    type Output = I::Output<O>;

    fn index(&self, index: I) -> &Self::Output {
        &self.inner[index]
    }
}

impl<O, S: ?Sized, D, T, I> IndexMut<I> for VecObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Vec<T>>,
    O: Observer<InnerDepth = Zero, Head = T>,
    I: SliceIndexImpl,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.inner[index]
    }
}

impl<T: Observe> Observe for Vec<T> {
    type Observer<'ob, S, D>
        = VecObserver<T::Observer<'ob, T, Zero>, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ref_observe! {
    impl [T] RefObserve for Vec<T>;
}

impl<T: Snapshot> Snapshot for Vec<T> {
    type Snapshot = Vec<T::Snapshot>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.iter().map(|item| item.to_snapshot()).collect()
    }

    fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
        self.len() == snapshot.len() && self.iter().zip(snapshot.iter()).all(|(a, b)| a.eq_snapshot(b))
    }
}

#[cfg(test)]
#[cfg(feature = "truncate")]
mod tests {
    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    // Known issue: overlapping Index calls with live references cause UB because the
    // second call's `Pointer::as_mut` retags the observed memory, invalidating provenance
    // held by observers returned from the first call. Fixing this requires Pointer to
    // support provenance refresh through shared access.
    // #[test]
    // fn overlapping_index_aliasing() {
    //     let mut vec: Vec<i32> = vec![1, 2, 3];
    //     let mut ob = vec.__observe();
    //     let a = &ob[0];
    //     let b = &ob[0];
    //     assert_eq!(*a, 1);
    //     assert_eq!(*b, 1);
    // }

    #[test]
    fn no_change_returns_none() {
        let mut vec: Vec<i32> = vec![];
        let mut ob = vec.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn deref_mut_triggers_replace() {
        let mut vec: Vec<i32> = vec![1];
        let mut ob = vec.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([]))));
    }

    #[test]
    fn push_on_empty_triggers_replace() {
        let mut vec: Vec<i32> = vec![];
        let mut ob = vec.__observe();
        ob.push(1);
        ob.push(2);
        ob.push(3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1, 2, 3]))));
    }

    #[test]
    fn push_triggers_append() {
        let mut vec: Vec<i32> = vec![1];
        let mut ob = vec.__observe();
        ob.push(2);
        ob.push(3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([2, 3]))));
    }

    #[test]
    fn append_vec() {
        let mut vec: Vec<i32> = vec![1];
        let mut ob = vec.__observe();
        let mut extra = vec![4, 5];
        ob.append(&mut extra);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([4, 5]))));
    }

    #[test]
    fn extend_from_slice() {
        let mut vec: Vec<i32> = vec![1];
        let mut ob = vec.__observe();
        ob.extend_from_slice(&[6, 7]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([6, 7]))));
    }

    #[test]
    fn index_by_usize_1() {
        let mut vec: Vec<i32> = vec![1, 2];
        let mut ob = vec.__observe();
        assert_eq!(ob[0], 1);
        ob.reserve(4); // force reallocation
        *ob[0].tracked_mut() = 99;
        ob.reserve(64); // force reallocation
        assert_eq!(ob[0], 99);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(-2, json!(99))));
    }

    #[test]
    fn index_by_usize_2() {
        let mut vec: Vec<i32> = vec![1, 2];
        let mut ob = vec.__observe();
        *ob[0].tracked_mut() = 99;
        ob.reserve(64); // force reallocation
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(-2, json!(99))));
    }

    #[test]
    fn append_and_index() {
        let mut vec: Vec<i32> = vec![1];
        let mut ob = vec.__observe();
        *ob[0].tracked_mut() = 11;
        ob.push(2);
        *ob[1].tracked_mut() = 12;
        let Json(mutation) = ob.flush().unwrap();
        // All existing elements (only ob[0]) report Replace, and there are appended elements.
        // The optimization collapses everything into a single whole-vec Replace.
        assert_eq!(mutation, Some(replace!(_, json!([11, 12]))));
    }

    #[test]
    fn non_replace_child_with_append() {
        let mut vec = vec!["hello".to_string(), "world".to_string()];
        let mut ob = vec.__observe();
        ob[0].push_str("!");
        ob.push("new".to_string());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, append!(_, json!(["new"])), append!(-3, json!("!"))))
        );
    }

    #[test]
    fn index_by_range() {
        let mut vec: Vec<i32> = vec![1, 2, 3, 4];
        let mut ob = vec.__observe();
        {
            let slice = &mut ob[1..];
            *slice[0].tracked_mut() = 222;
            *slice[1].tracked_mut() = 333;
        }
        assert_eq!(ob, vec![1, 222, 333, 4]);
        assert_eq!(&ob[..], &[1, 222, 333, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(-2, json!(333)), replace!(-3, json!(222))))
        )
    }

    #[test]
    fn pop_all_then_push_triggers_replace() {
        let mut vec = vec![1, 2];
        let mut ob = vec.__observe();
        ob.pop();
        ob.pop();
        ob.push(3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([3]))));
    }

    #[test]
    fn pop_push_clears_stale_state() {
        let mut vec = vec!["a".to_string(), "b".to_string(), "ab".to_string()];
        let mut ob = vec.__observe();

        // Modify element 2, then pop and push back in the SAME cycle.
        // The inner observer Vec never sees a shorter length, so resize_with
        // alone cannot clear the stale state — flush must reset it.
        ob[2].truncate(1);
        ob.pop();
        ob.push("cd".to_string());
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some()); // Truncate(1) + Append(["cd"])

        // Next cycle: element 2 should have a fresh observer.
        assert_eq!(ob[2], "cd");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn swap_remove_triggers_truncate() {
        let mut vec = vec![1, 2, 3, 4];
        let mut ob = vec.__observe();
        let removed = ob.swap_remove(1);
        assert_eq!(removed, 2);
        assert_eq!(ob, vec![1, 4, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 3), append!(_, json!([4, 3])))));
    }

    #[test]
    fn insert_triggers_truncate() {
        let mut vec = vec![1, 2, 3];
        let mut ob = vec.__observe();
        ob.insert(1, 99);
        assert_eq!(ob, vec![1, 99, 2, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 2), append!(_, json!([99, 2, 3]))))
        );
    }

    #[test]
    fn remove_triggers_truncate() {
        let mut vec = vec![1, 2, 3, 4];
        let mut ob = vec.__observe();
        let removed = ob.remove(1);
        assert_eq!(removed, 2);
        assert_eq!(ob, vec![1, 3, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 3), append!(_, json!([3, 4])))));
    }

    #[test]
    fn retain_removes_elements() {
        let mut vec = vec![1, 2, 3, 4];
        let mut ob = vec.__observe();
        ob.retain(|x| x % 2 == 1);
        assert_eq!(ob, vec![1, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 3), append!(_, json!([3])))));
    }

    #[test]
    fn retain_mut_removes_elements() {
        let mut vec = vec![1, 2, 3, 4, 5];
        let mut ob = vec.__observe();
        ob.retain_mut(|x| *x % 2 == 1);
        assert_eq!(ob, vec![1, 3, 5]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!([3, 5])))));
    }

    #[test]
    fn retain_no_removal() {
        let mut vec = vec![1, 2, 3];
        let mut ob = vec.__observe();
        ob.retain(|_| true);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn drain_range_triggers_truncate() {
        let mut vec = vec![1, 2, 3, 4];
        let mut ob = vec.__observe();
        let drained: Vec<_> = ob.drain(1..3).collect();
        assert_eq!(drained, vec![2, 3]);
        assert_eq!(ob, vec![1, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 3), append!(_, json!([4])))));
    }

    #[test]
    fn drain_all_triggers_replace() {
        let mut vec = vec![1, 2, 3];
        let mut ob = vec.__observe();
        let _: Vec<_> = ob.drain(..).collect();
        assert_eq!(ob, Vec::<i32>::new());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([]))));
    }

    #[test]
    fn splice_triggers_truncate() {
        let mut vec = vec![1, 2, 3, 4];
        let mut ob = vec.__observe();
        let removed: Vec<_> = ob.splice(1..3, [10, 20, 30]).collect();
        assert_eq!(removed, vec![2, 3]);
        assert_eq!(ob, vec![1, 10, 20, 30, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 3), append!(_, json!([10, 20, 30, 4]))))
        );
    }

    #[test]
    fn extract_if_no_match() {
        let mut vec = vec![1, 2, 3];
        let mut ob = vec.__observe();
        let extracted: Vec<_> = ob.extract_if(.., |x| *x > 10).collect();
        assert!(extracted.is_empty());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn extract_if_after_append_index() {
        let mut vec = vec![1, 2];
        let mut ob = vec.__observe();
        ob.push(3);
        ob.push(4);
        // Range starts at append_index, so extraction is fully untracked.
        let extracted: Vec<_> = ob.extract_if(2.., |x| *x > 2).collect();
        assert_eq!(extracted, vec![3, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn extract_if_before_append_index() {
        let mut vec = vec![1, 2, 3, 4];
        let mut ob = vec.__observe();
        // First extracted element is at index 1, so mark_truncate(1).
        let extracted: Vec<_> = ob.extract_if(.., |x| *x % 2 == 0).collect();
        assert_eq!(extracted, vec![2, 4]);
        assert_eq!(ob, vec![1, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 3), append!(_, json!([3])))));
    }

    #[test]
    fn extract_if_preserves_prefix() {
        let mut vec = vec![1, 2, 3, 4, 5];
        let mut ob = vec.__observe();
        // Only the last element matches; mark_truncate(4) preserves elements 0-3.
        let extracted: Vec<_> = ob.extract_if(.., |x| *x == 5).collect();
        assert_eq!(extracted, vec![5]);
        assert_eq!(ob, vec![1, 2, 3, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 1)));
    }

    #[test]
    fn extract_if_straddles_append_index() {
        let mut vec = vec![1, 2, 3];
        let mut ob = vec.__observe();
        ob.push(4);
        ob.push(5);
        // Range 1.. straddles: start_index=1 < append_index=3.
        // First extraction triggers mark_truncate(1).
        let extracted: Vec<_> = ob.extract_if(1.., |x| *x % 2 == 0).collect();
        assert_eq!(extracted, vec![2, 4]);
        assert_eq!(ob, vec![1, 3, 5]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 2), append!(_, json!([3, 5])),)));
    }

    #[rustversion::since(1.95)]
    #[test]
    fn push_mut_returns_observer() {
        let mut vec: Vec<String> = vec!["a".into()];
        let mut ob = vec.__observe();
        let pushed = ob.push_mut("b".into());
        pushed.push_str("c");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["bc"]))));
    }

    #[rustversion::since(1.95)]
    #[test]
    fn push_mut_on_empty_triggers_replace() {
        let mut vec: Vec<String> = vec![];
        let mut ob = vec.__observe();
        let pushed = ob.push_mut("x".into());
        pushed.push_str("y");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(["xy"]))));
    }

    #[rustversion::since(1.95)]
    #[test]
    fn insert_mut_returns_observer() {
        let mut vec: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let mut ob = vec.__observe();
        let inserted = ob.insert_mut(1, "X".into());
        inserted.push_str("Y");
        assert_eq!(ob, vec!["a".to_string(), "XY".into(), "b".into(), "c".into()]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 2), append!(_, json!(["XY", "b", "c"]))))
        );
    }

    #[rustversion::since(1.95)]
    #[test]
    fn insert_mut_at_end_acts_like_push() {
        let mut vec: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let mut ob = vec.__observe();
        let inserted = ob.insert_mut(3, "d".into());
        inserted.push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["d!"]))));
    }
}

#[cfg(test)]
#[cfg(not(feature = "truncate"))]
mod tests_no_truncate {
    use serde_json::json;

    use crate::adapter::Json;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn modify_then_pop_no_stale_leak() {
        let mut vec = vec!["a".to_string(), "b".to_string()];
        let mut ob = vec.__observe();
        ob[0].push_str("!");
        ob.pop();
        let Json(m1) = ob.flush().unwrap();
        assert_eq!(m1, Some(muon_test_utils::replace!(_, json!(["a!"]))));
        let Json(m2) = ob.flush().unwrap();
        assert_eq!(m2, None);
    }
}
