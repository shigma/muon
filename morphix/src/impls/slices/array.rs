//! Observer implementation for arrays `[T; N]`.

use std::fmt::Debug;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice::SliceIndex;

use serde::Serialize;

use crate::general::Snapshot;
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::impls::slice::{SliceObserver, SliceObserverState, SliceRefObserverState, SliceSerializeObserverState};
use crate::observe::{DefaultSpec, Observer, RefObserve, RefObserver, SerializeObserver};
use crate::{Mutations, Observe};

impl<O, const N: usize> Invalidate<[O::Head; N]> for [O; N]
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
{
    /// Unlike [`UnsafeCell<Vec<O>>`](core::cell::UnsafeCell) which clears its storage on `DerefMut`
    /// (producing a full [`Replace`](crate::MutationKind::Replace)), the array implementation
    /// triggers [`as_deref_mut_coinductive()`][as_deref_mut_coinductive] on each element,
    /// preserving per-element granularity. This is appropriate because arrays have a fixed,
    /// typically small length — the resulting batch of per-element mutations is bounded and
    /// comparable in size to a whole-array [`Replace`](crate::MutationKind::Replace), while
    /// unchanged elements can be filtered out by the element observer (e.g.,
    /// [`SnapshotObserver`](crate::general::SnapshotObserver)).
    ///
    /// [as_deref_mut_coinductive]: crate::helper::AsDerefMutCoinductive::as_deref_mut_coinductive
    fn invalidate(&mut self, _: &[O::Head; N]) {
        for ob in self.as_mut_slice() {
            O::invalidate(ob);
        }
    }
}

impl<O, const N: usize> SliceObserverState for [O; N]
where
    O: Observer<InnerDepth = Zero, Head: Sized>,
{
    type Target = [O::Head; N];
    type Item = O;

    fn as_slice(&self) -> &[O] {
        self
    }

    fn as_mut_slice(&mut self) -> &mut [O] {
        self
    }

    fn observe(slice: &mut Self::Target) -> Self {
        slice.each_mut().map(O::observe)
    }

    unsafe fn relocate(&self, _slice: &mut Self::Target) {
        // No need to re-initialize fixed-size array.
    }
}

impl<O, const N: usize> SliceRefObserverState for [O; N]
where
    O: RefObserver<InnerDepth = Zero, Head: Sized>,
{
    type Target = [O::Head; N];
    type Item = O;

    fn observe(slice: &Self::Target) -> Self {
        slice.each_ref().map(O::observe)
    }
}

impl<O, const N: usize, S, D> SliceSerializeObserverState<S, D> for [O; N]
where
    D: Unsigned,
    S: AsDeref<D, Target = [O::Head; N]> + ?Sized,
    O: SerializeObserver<InnerDepth = Zero>,
    O::Head: Serialize + Sized + 'static,
{
    type Target = [O::Head; N];
    fn flush(&mut self, ptr: &mut Pointer<S>) -> Mutations {
        let slice = (**ptr).as_deref();
        let mut mutations = Mutations::new();
        let mut is_replace = true;
        for (index, ob) in self.iter_mut().enumerate() {
            let inner_mutations = unsafe { SerializeObserver::flush(ob) };
            is_replace &= inner_mutations.is_replace();
            mutations.insert(index, inner_mutations);
        }
        if is_replace {
            return Mutations::replace(slice.as_ref());
        };
        mutations
    }
}

/// Observer implementation for arrays `[T; N]`.
pub struct ArrayObserver<const N: usize, O, S: ?Sized, D = Zero> {
    inner: SliceObserver<[O; N], S, D>,
}

impl<const N: usize, O, S: ?Sized, D, T> ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = [T; N]>,
    O: Observer<InnerDepth = Zero, Head = T>,
{
    /// See [`array::as_mut_slice`].
    pub fn as_mut_slice(&mut self) -> &mut [O] {
        self.inner.force_mut()
    }

    /// See [`array::each_mut`].
    pub fn each_mut(&mut self) -> [&mut O; N] {
        self.inner.force_mut();
        self.inner.state.each_mut()
    }
}

impl<const N: usize, O, S: ?Sized, D> Deref for ArrayObserver<N, O, S, D> {
    type Target = SliceObserver<[O; N], S, D>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<const N: usize, O, S: ?Sized, D> DerefMut for ArrayObserver<N, O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<const N: usize, O, S: ?Sized, D> QuasiObserver for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = [O::Head; N]>,
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
{
    type Head = S;
    type OuterDepth = Succ<Succ<Zero>>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        SliceObserver::invalidate(&mut this.inner);
    }
}

impl<const N: usize, O, S: ?Sized, D, T> Observer for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = [T; N]>,
    O: Observer<InnerDepth = Zero, Head = T>,
{
    fn observe(head: &mut Self::Head) -> Self {
        Self {
            inner: Observer::observe(head),
        }
    }

    unsafe fn relocate(this: &mut Self, head: &mut Self::Head) {
        unsafe { Observer::relocate(&mut this.inner, head) }
    }
}

impl<const N: usize, O, S: ?Sized, D, T> RefObserver for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = [T; N]>,
    O: RefObserver<InnerDepth = Zero, Head = T>,
{
    fn observe(head: &Self::Head) -> Self {
        Self {
            inner: RefObserver::observe(head),
        }
    }

    unsafe fn relocate(this: &mut Self, head: &Self::Head) {
        unsafe { RefObserver::relocate(&mut this.inner, head) }
    }
}

impl<const N: usize, O, S: ?Sized, D, T> SerializeObserver for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = [T; N]>,
    O: SerializeObserver<InnerDepth = Zero, Head = T>,
    T: Serialize + 'static,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        unsafe { SliceObserver::flush(&mut this.inner) }
    }
}

impl<const N: usize, O, S: ?Sized, D> Debug for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = [O::Head; N]>,
    O: Observer<InnerDepth = Zero, Head: Sized + Debug>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ArrayObserver").field(&self.untracked_ref()).finish()
    }
}

macro_rules! generic_impl_partial_eq {
    ($(impl $([$($gen:tt)*])? PartialEq<$ty:ty> for [_; N]);* $(;)?) => {
        $(
            impl<$($($gen)*,)? const N: usize, O, S: ?Sized, D> PartialEq<$ty> for ArrayObserver<N, O, S, D>
            where
                D: Unsigned,
                S: AsDeref<D, Target = [O::Head; N]>,
                O: Observer<InnerDepth = Zero, Head: Sized>,
                [O::Head; N]: PartialEq<$ty>,
            {
                fn eq(&self, other: &$ty) -> bool {
                    self.untracked_ref().eq(other)
                }
            }
        )*
    };
}

generic_impl_partial_eq! {
    impl [U] PartialEq<[U; N]> for [_; N];
    impl [U] PartialEq<[U]> for [_; N];
    impl ['a, U] PartialEq<&'a U> for [_; N];
    impl ['a, U] PartialEq<&'a mut U> for [_; N];
}

impl<const N: usize, O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<ArrayObserver<N, O2, S2, D2>>
    for ArrayObserver<N, O1, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    O1: Observer<InnerDepth = Zero, Head: Sized>,
    O2: Observer<InnerDepth = Zero, Head: Sized>,
    S1: AsDeref<D1, Target = [O1::Head; N]>,
    S2: AsDeref<D2, Target = [O2::Head; N]>,
    [O1::Head; N]: PartialEq<[O2::Head; N]>,
{
    fn eq(&self, other: &ArrayObserver<N, O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<const N: usize, O, S: ?Sized, D> Eq for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = [O::Head; N]>,
    O: Observer<InnerDepth = Zero, Head: Sized + Eq>,
{
}

impl<const N: usize, O, S: ?Sized, D, U> PartialOrd<[U; N]> for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = [O::Head; N]>,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    [O::Head; N]: PartialOrd<[U; N]>,
{
    fn partial_cmp(&self, other: &[U; N]) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other)
    }
}

impl<const N: usize, O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<ArrayObserver<N, O2, S2, D2>>
    for ArrayObserver<N, O1, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    O1: Observer<InnerDepth = Zero, Head: Sized>,
    O2: Observer<InnerDepth = Zero, Head: Sized>,
    S1: AsDeref<D1, Target = [O1::Head; N]>,
    S2: AsDeref<D2, Target = [O2::Head; N]>,
    [O1::Head; N]: PartialOrd<[O2::Head; N]>,
{
    fn partial_cmp(&self, other: &ArrayObserver<N, O2, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<const N: usize, O, S: ?Sized, D> Ord for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = [O::Head; N]>,
    O: Observer<InnerDepth = Zero, Head: Sized + Ord>,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

impl<const N: usize, O, S: ?Sized, D, T, I> Index<I> for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = [T; N]>,
    O: Observer<InnerDepth = Zero, Head = T>,
    I: SliceIndex<[O]>,
{
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        &self.inner[index]
    }
}

impl<const N: usize, O, S: ?Sized, D, T, I> IndexMut<I> for ArrayObserver<N, O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = [T; N]>,
    O: Observer<InnerDepth = Zero, Head = T>,
    I: SliceIndex<[O]>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.inner[index]
    }
}

impl<T: Observe, const N: usize> Observe for [T; N] {
    type Observer<'ob, S, D>
        = ArrayObserver<N, T::Observer<'ob, T, Zero>, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl<T: RefObserve, const N: usize> RefObserve for [T; N] {
    type Observer<'ob, S, D>
        = ArrayObserver<N, T::Observer<'ob, T, Zero>, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl<T: Snapshot, const N: usize> Snapshot for [T; N] {
    type Snapshot = [T::Snapshot; N];

    fn to_snapshot(&self) -> Self::Snapshot {
        std::array::from_fn(|i| self[i].to_snapshot())
    }

    fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
        (0..N).all(|i| self[i].eq_snapshot(&snapshot[i]))
    }
}

#[cfg(test)]
mod tests {
    use morphix_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_change_returns_none() {
        let mut arr = [1u32, 2, 3];
        let mut ob = arr.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn index_by_usize() {
        let mut arr = [10u32, 20, 30];
        let mut ob = arr.__observe();
        assert_eq!(ob[1], 20);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
        *ob[1].tracked_mut() = 99;
        assert_eq!(ob[1], 99);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(1, json!(99))));
    }

    #[test]
    fn multiple_index_mutations() {
        let mut arr = [1u32, 2, 3];
        let mut ob = arr.__observe();
        *ob[0].tracked_mut() = 10;
        *ob[2].tracked_mut() = 30;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(0, json!(10)), replace!(2, json!(30)))),
        );
    }

    #[test]
    fn deref_mut_triggers_replace() {
        let mut arr = [1u32, 2, 3];
        let mut ob = arr.__observe();
        *ob.tracked_mut() = [4, 5, 6];
        let Json(mutation) = ob.flush().unwrap();
        // DerefMut on array: all elements changed, so the optimization collapses into a single
        // whole-array Replace instead of a batch of per-element mutations.
        assert_eq!(mutation, Some(replace!(_, json!([4, 5, 6]))));
    }

    #[test]
    fn deref_mut_same_value_returns_none() {
        let mut arr = [1u32, 2, 3];
        let mut ob = arr.__observe();
        *ob.tracked_mut() = [1, 2, 3];
        let Json(mutation) = ob.flush().unwrap();
        // ShallowObserver detects no change on each element.
        assert_eq!(mutation, None);
    }

    #[test]
    fn swap() {
        let mut arr = [10u32, 20, 30];
        let mut ob = arr.__observe();
        ob.swap(0, 2);
        assert_eq!(ob, [30, 20, 10]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(0, json!(30)), replace!(2, json!(10)))),
        );
    }

    #[test]
    fn nested_string_append() {
        let mut arr = ["hello".to_string(), "world".to_string()];
        let mut ob = arr.__observe();
        ob[0].push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(0, json!("!"))));
    }

    #[test]
    fn flush_resets_state() {
        let mut arr = ["a".to_string(), "b".to_string()];
        let mut ob = arr.__observe();
        ob[0].push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        // Second flush with no new changes returns None.
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_none(), "expected None, got {mutation:?}");
    }
}
