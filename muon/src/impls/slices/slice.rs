//! Observer implementation for slices `[T]`.

use std::cmp::Ordering;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut, Index, IndexMut, Range, RangeBounds};
use std::slice::{
    ChunkByMut, ChunksExactMut, ChunksMut, GetDisjointMutError, IterMut, RChunksExactMut, RChunksMut, RSplitMut,
    RSplitNMut, SplitInclusiveMut, SplitMut, SplitNMut,
};

use crate::general::{Unsize, UnsizeObserver};
use crate::helper::macros::delegate_methods;
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::impls::slices::helper::{GetDisjointMutIndexImpl, SliceIndexImpl};
use crate::impls::slices::range_set::RangeSet;
use crate::impls::vec::VecObserverState;
use crate::observe::{DefaultSpec, Observer, RefObserve, RefObserver, SerializeObserver};
use crate::{Mutations, Observe};

/// Trait for managing the internal observer storage within a slice observer.
///
/// This trait abstracts over the storage and initialization of element observers, allowing
/// [`SliceObserver`] to lazily create observers for individual elements as they are accessed.
pub trait SliceObserverState: Invalidate<Self::Target> + Sized {
    /// The slice-like type being observed.
    type Target: AsRef<[<Self::Item as QuasiObserver>::Head]> + ?Sized;

    /// The element [`Observer`] type.
    type Item: Observer<InnerDepth = Zero, Head: Sized>;

    /// Creates an [`Observer`] collection for the given slice.
    fn observe(slice: &mut Self::Target) -> Self;

    /// Ensures the observer(s) at `index` are initialized and returns a shared reference.
    fn get<I: SliceIndexImpl>(&self, index: I, slice: &mut Self::Target) -> Option<&I::Output<Self::Item>>;

    /// Ensures the observer(s) at `index` are initialized and returns a mutable reference.
    fn get_mut<I: SliceIndexImpl>(&mut self, index: I, slice: &mut Self::Target) -> Option<&mut I::Output<Self::Item>>;
}

/// Shared-reference counterpart to [`SliceObserverState`] for element [`RefObserver`] management.
pub trait SliceRefObserverState: Invalidate<Self::Target> + Sized {
    /// The slice-like type being observed.
    type Target: AsRef<[<Self::Item as QuasiObserver>::Head]> + ?Sized;
    /// The element [`RefObserver`] type.
    type Item: RefObserver<InnerDepth = Zero, Head: Sized>;

    /// Creates an [`RefObserver`] collection for the given slice.
    fn observe(slice: &Self::Target) -> Self;
}

/// Flush logic for slice-backed observer state, parameterized by `S` and `D`.
///
/// This trait is generic over the head type `S` and depth `D`, allowing each implementor to
/// choose its own mutability requirement: [`[O; N]`](prim@array) bounds `S: AsDeref<D>` (shared
/// access), while [`VecObserverState`] bounds `S: AsDerefMut<D>` (mutable access for element
/// relocation).
pub trait SliceSerializeObserverState<S: ?Sized, D>: Invalidate<Self::Target> {
    /// The slice-like type being observed.
    type Target: ?Sized;

    /// Consumes the accumulated mutation state, flushes inner element observers, and returns the
    /// collected [`Mutations`].
    ///
    /// This method must fully reset all internal state so that an immediately subsequent call with
    /// no intervening mutations returns empty.
    fn flush(&mut self, ptr: &mut Pointer<S>) -> Mutations;
}

/// Lazily-initialized element observer storage backed by [`MaybeUninit`].
///
/// Only indices tracked by `initialized` contain valid observers. The `data` vector is always sized
/// to match the observed slice length, with unaccessed slots left as [`MaybeUninit::uninit()`].
pub(crate) struct LazyVec<O> {
    pub data: Vec<MaybeUninit<O>>,
    pub initialized: RangeSet,
}

impl<O> LazyVec<O> {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            initialized: RangeSet::new(),
        }
    }

    pub fn truncate(&mut self, new_len: usize) {
        if new_len >= self.data.len() {
            return;
        }
        for range in self.initialized.overlapping(&(new_len..self.data.len())) {
            for i in range.start.max(new_len)..range.end {
                unsafe { self.data[i].assume_init_drop() };
            }
        }
        self.initialized.remove(new_len..self.data.len());
        self.data.truncate(new_len);
    }
}

impl<O> Drop for LazyVec<O> {
    fn drop(&mut self) {
        for range in self.initialized.iter() {
            for i in range.clone() {
                unsafe { self.data[i].assume_init_drop() };
            }
        }
    }
}

impl<O> LazyVec<O>
where
    O: Observer<InnerDepth = Zero, Head: Sized>,
{
    pub fn relocate(&mut self, range: Range<usize>, slice: &mut [O::Head]) {
        // Ensure data is sized to match the slice
        if self.data.len() < slice.len() {
            self.data.resize_with(slice.len(), MaybeUninit::uninit);
        }
        if self.data.len() > slice.len() {
            self.truncate(slice.len());
        }
        if range.is_empty() {
            return;
        }
        for gap in self.initialized.gaps(&range) {
            for i in gap {
                self.data[i] = MaybeUninit::new(O::observe(&mut slice[i]));
            }
        }
        for existing in self.initialized.overlapping(&range) {
            for i in existing.clone() {
                unsafe { Observer::relocate(self.data[i].assume_init_mut(), &mut slice[i]) }
            }
        }
        self.initialized.insert(range);
    }
}

/// Observer implementation for slices `[T]`.
pub struct SliceObserver<V, S: ?Sized, D = Zero> {
    pub(super) ptr: Pointer<S>,
    pub(super) state: V,
    phantom: PhantomData<D>,
}

impl<V, S: ?Sized, D> Deref for SliceObserver<V, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<V, S: ?Sized, D> DerefMut for SliceObserver<V, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.ptr);
        &mut self.ptr
    }
}

impl<V, S: ?Sized, D, T: ?Sized> QuasiObserver for SliceObserver<V, S, D>
where
    V: Invalidate<T>,
    D: Unsigned,
    S: AsDeref<D, Target = T>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        Invalidate::invalidate(&mut this.state, (*this.ptr).as_deref());
    }
}

impl<V, S: ?Sized, D, T> Observer for SliceObserver<V, S, D>
where
    V: SliceObserverState,
    V::Item: Observer<InnerDepth = Zero, Head = T>,
    D: Unsigned,
    S: AsDerefMut<D, Target = V::Target>,
{
    fn observe(head: &mut Self::Head) -> Self {
        let this = Self {
            state: V::observe(head.as_deref_mut()),
            ptr: Pointer::new(head),
            phantom: PhantomData,
        };
        Pointer::register_state::<_, D>(&this.ptr, &this.state);
        this
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe { Pointer::set_unchecked(this, head) };
    }
}

impl<V, S: ?Sized, D, T> RefObserver for SliceObserver<V, S, D>
where
    V: SliceRefObserverState,
    V::Item: RefObserver<InnerDepth = Zero, Head = T>,
    D: Unsigned,
    S: AsDeref<D, Target = V::Target>,
{
    fn observe(head: &Self::Head) -> Self {
        let this = Self {
            ptr: Pointer::new(head),
            state: V::observe(head.as_deref()),
            phantom: PhantomData,
        };
        Pointer::register_state::<_, D>(&this.ptr, &this.state);
        this
    }

    unsafe fn relocate(this: &mut Self, head: *const Self::Head) {
        unsafe { Pointer::set_unchecked(this, head) };
    }
}

impl<V, S: ?Sized, D> SerializeObserver for SliceObserver<V, S, D>
where
    V: SliceSerializeObserverState<S, D>,
    D: Unsigned,
    S: AsDeref<D, Target = V::Target>,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        this.state.flush(&mut this.ptr)
    }
}

#[expect(clippy::type_complexity)]
impl<V, S: ?Sized, D, T> SliceObserver<V, S, D>
where
    V: SliceObserverState,
    V::Item: Observer<InnerDepth = Zero, Head = T>,
    D: Unsigned,
    S: AsDerefMut<D, Target = V::Target>,
    S::Target: AsMut<[T]>,
{
    pub(crate) fn force_mut(&mut self) -> &mut [V::Item] {
        let slice = (*self.ptr).as_deref_mut();
        self.state.get_mut(.., slice).expect("index out of bounds")
    }

    fn nonempty_mut(&mut self) -> &mut [T] {
        if (*self).untracked_ref().as_ref().is_empty() {
            self.untracked_mut().as_mut()
        } else {
            self.tracked_mut().as_mut()
        }
    }

    /// See [`slice::first_mut`].
    pub fn first_mut(&mut self) -> Option<&mut V::Item> {
        let slice = (*self.ptr).as_deref_mut();
        self.state.get_mut(0usize, slice)
    }

    /// See [`slice::last_mut`].
    pub fn last_mut(&mut self) -> Option<&mut V::Item> {
        let slice = (*self.ptr).as_deref_mut();
        let len = slice.as_ref().len();
        if len == 0 {
            None
        } else {
            self.state.get_mut(len - 1, slice)
        }
    }

    /// See [`slice::first_chunk_mut`].
    pub fn first_chunk_mut<const N: usize>(&mut self) -> Option<&mut [V::Item; N]> {
        let slice = (*self.ptr).as_deref_mut();
        self.state.get_mut(0..N, slice)?.try_into().ok()
    }

    delegate_methods! { force_mut() as slice =>
        pub fn split_first_chunk_mut<const N: usize>(&mut self) -> Option<(&mut [V::Item; N], &mut [V::Item])>;
        pub fn split_last_chunk_mut<const N: usize>(&mut self) -> Option<(&mut [V::Item], &mut [V::Item; N])>;
    }

    /// See [`slice::last_chunk_mut`].
    pub fn last_chunk_mut<const N: usize>(&mut self) -> Option<&mut [V::Item; N]> {
        let slice = (*self.ptr).as_deref_mut();
        let len = slice.as_ref().len();
        if len < N {
            None
        } else {
            self.state.get_mut(len - N..len, slice)?.try_into().ok()
        }
    }

    /// See [`slice::get_mut`].
    pub fn get_mut<I: SliceIndexImpl>(&mut self, index: I) -> Option<&mut I::Output<V::Item>> {
        let slice = (*self.ptr).as_deref_mut();
        self.state.get_mut(index, slice)
    }

    /// See [`slice::get_unchecked_mut`].
    ///
    /// ## Safety
    ///
    /// See [`slice::get_unchecked_mut`] for safety requirements.
    pub unsafe fn get_unchecked_mut<I: SliceIndexImpl>(&mut self, index: I) -> &mut I::Output<V::Item> {
        unsafe { self.get_mut(index).unwrap_unchecked() }
    }

    delegate_methods! { force_mut() as slice =>
        pub fn as_mut_ptr(&mut self) -> *mut V::Item;
        pub fn as_mut_ptr_range(&mut self) -> Range<*mut V::Item>;
        #[rustversion::since(1.93)]
        pub fn as_mut_array<const N: usize>(&mut self) -> Option<&mut [V::Item; N]>;
    }

    /// See [`slice::swap`].
    pub fn swap(&mut self, a: usize, b: usize) {
        QuasiObserver::invalidate(&mut self[a]);
        QuasiObserver::invalidate(&mut self[b]);
        self.untracked_mut().as_mut().swap(a, b);
    }

    delegate_methods! { nonempty_mut() as slice =>
        pub fn reverse(&mut self);
    }

    delegate_methods! { force_mut() as slice =>
        pub fn iter_mut(&mut self) -> IterMut<'_, V::Item>;
        pub fn chunks_mut(&mut self, chunk_size: usize) -> ChunksMut<'_, V::Item>;
        pub fn chunks_exact_mut(&mut self, chunk_size: usize) -> ChunksExactMut<'_, V::Item>;
        pub unsafe fn as_chunks_unchecked_mut<const N: usize>(&mut self) -> &mut [[V::Item; N]];
        pub fn as_chunks_mut<const N: usize>(&mut self) -> (&mut [[V::Item; N]], &mut [V::Item]);
        pub fn as_rchunks_mut<const N: usize>(&mut self) -> (&mut [V::Item], &mut [[V::Item; N]]);
        pub fn rchunks_mut(&mut self, chunk_size: usize) -> RChunksMut<'_, V::Item>;
        pub fn rchunks_exact_mut(&mut self, chunk_size: usize) -> RChunksExactMut<'_, V::Item>;
        pub fn chunk_by_mut<F>(&mut self, pred: F) -> ChunkByMut<'_, V::Item, F> where F: FnMut(&V::Item, &V::Item) -> bool;
        pub fn split_at_mut(&mut self, mid: usize) -> (&mut [V::Item], &mut [V::Item]);
        pub unsafe fn split_at_mut_unchecked(&mut self, mid: usize) -> (&mut [V::Item], &mut [V::Item]);
        pub fn split_at_mut_checked(&mut self, mid: usize) -> Option<(&mut [V::Item], &mut [V::Item])>;
        pub fn split_mut<F>(&mut self, pred: F) -> SplitMut<'_, V::Item, F> where F: FnMut(&V::Item) -> bool;
        pub fn split_inclusive_mut<F>(&mut self, pred: F) -> SplitInclusiveMut<'_, V::Item, F> where F: FnMut(&V::Item) -> bool;
        pub fn rsplit_mut<F>(&mut self, pred: F) -> RSplitMut<'_, V::Item, F> where F: FnMut(&V::Item) -> bool;
        pub fn splitn_mut<F>(&mut self, n: usize, pred: F) -> SplitNMut<'_, V::Item, F> where F: FnMut(&V::Item) -> bool;
        pub fn rsplitn_mut<F>(&mut self, n: usize, pred: F) -> RSplitNMut<'_, V::Item, F> where F: FnMut(&V::Item) -> bool;
    }

    delegate_methods! { nonempty_mut() as slice =>
        pub fn sort_unstable(&mut self) where T: Ord;
        pub fn sort_unstable_by<F>(&mut self, compare: F) where F: FnMut(&T, &T) -> Ordering;
        pub fn sort_unstable_by_key<K, F>(&mut self, f: F) where F: FnMut(&T) -> K, K: Ord;
        pub fn select_nth_unstable(&mut self, index: usize) -> (&mut [T], &mut T, &mut [T]) where T: Ord;
        pub fn select_nth_unstable_by<F>(&mut self, index: usize, compare: F) -> (&mut [T], &mut T, &mut [T]) where F: FnMut(&T, &T) -> Ordering;
        pub fn select_nth_unstable_by_key<K, F>(&mut self, index: usize, f: F) -> (&mut [T], &mut T, &mut [T]) where F: FnMut(&T) -> K, K: Ord;
        pub fn rotate_left(&mut self, mid: usize);
        pub fn rotate_right(&mut self, k: usize);
        pub fn fill(&mut self, value: T) where T: Clone;
        pub fn fill_with<F>(&mut self, f: F) where F: FnMut() -> T;
        pub fn clone_from_slice(&mut self, src: &[T]) where T: Clone;
        pub fn copy_from_slice(&mut self, src: &[T]) where T: Copy;
        pub fn copy_within<R: RangeBounds<usize>>(&mut self, src: R, dest: usize) where T: Copy;
        pub fn swap_with_slice(&mut self, other: &mut [T]);
        pub unsafe fn align_to_mut<U>(&mut self) -> (&mut [T], &mut [U], &mut [T]);
    }

    /// See [`slice::get_disjoint_unchecked_mut`].
    ///
    /// ## Safety
    ///
    /// See [`slice::get_disjoint_unchecked_mut`] for safety requirements.
    pub unsafe fn get_disjoint_unchecked_mut<I, const N: usize>(&mut self, indices: [I; N]) -> [&mut I::Output; N]
    where
        I: GetDisjointMutIndexImpl<V::Item>,
    {
        unsafe { GetDisjointMutIndexImpl::get_disjoint_unchecked_mut(self.force_mut(), indices) }
    }

    /// See [`slice::get_disjoint_unchecked_mut`].
    pub fn get_disjoint_mut<I, const N: usize>(
        &mut self,
        indices: [I; N],
    ) -> Result<[&mut I::Output; N], GetDisjointMutError>
    where
        I: GetDisjointMutIndexImpl<V::Item>,
    {
        GetDisjointMutIndexImpl::get_disjoint_mut(self.force_mut(), indices)
    }

    delegate_methods! { nonempty_mut() as slice =>
        pub fn sort(&mut self) where T: Ord;
        pub fn sort_by<F>(&mut self, compare: F) where F: FnMut(&T, &T) -> Ordering;
        pub fn sort_by_key<K, F>(&mut self, f: F) where F: FnMut(&T) -> K, K: Ord;
        pub fn sort_by_cached_key<K, F>(&mut self, f: F) where F: FnMut(&T) -> K, K: Ord;
    }
}

impl<V, S: ?Sized, D> Debug for SliceObserver<V, S, D>
where
    V: SliceObserverState,
    D: Unsigned,
    S: AsDeref<D, Target = V::Target>,
    V::Target: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SliceObserver").field(&self.untracked_ref()).finish()
    }
}

impl<V1, V2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<SliceObserver<V2, S2, D2>> for SliceObserver<V1, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    V1: SliceObserverState,
    V2: SliceObserverState,
    S1: AsDeref<D1, Target = V1::Target>,
    S2: AsDeref<D2, Target = V2::Target>,
    V1::Target: PartialEq<V2::Target>,
{
    fn eq(&self, other: &SliceObserver<V2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<V, S: ?Sized, D> Eq for SliceObserver<V, S, D>
where
    D: Unsigned,
    V: SliceObserverState,
    S: AsDeref<D, Target = V::Target>,
    V::Target: Eq,
{
}

impl<V1, V2, S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<SliceObserver<V2, S2, D2>> for SliceObserver<V1, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    V1: SliceObserverState,
    V2: SliceObserverState,
    S1: AsDeref<D1, Target = V1::Target>,
    S2: AsDeref<D2, Target = V2::Target>,
    V1::Target: PartialOrd<V2::Target>,
{
    fn partial_cmp(&self, other: &SliceObserver<V2, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<V, S: ?Sized, D> Ord for SliceObserver<V, S, D>
where
    D: Unsigned,
    V: SliceObserverState,
    S: AsDeref<D, Target = V::Target>,
    V::Target: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

impl<V, S: ?Sized, D, T, I> Index<I> for SliceObserver<V, S, D>
where
    V: SliceObserverState,
    D: Unsigned,
    S: AsDerefMut<D, Target = V::Target>,
    V::Item: Observer<InnerDepth = Zero, Head = T>,
    I: SliceIndexImpl,
{
    type Output = I::Output<V::Item>;

    fn index(&self, index: I) -> &Self::Output {
        let slice = unsafe { Pointer::as_mut(&self.ptr).as_deref_mut() };
        self.state.get(index, slice).expect("index out of bounds")
    }
}

impl<V, S: ?Sized, D, T, I> IndexMut<I> for SliceObserver<V, S, D>
where
    V: SliceObserverState,
    D: Unsigned,
    S: AsDerefMut<D, Target = V::Target>,
    S::Target: AsMut<[T]>,
    V::Item: Observer<InnerDepth = Zero, Head = T>,
    I: SliceIndexImpl,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        let slice = (*self.ptr).as_deref_mut();
        self.state.get_mut(index, slice).expect("index out of bounds")
    }
}

impl<T: Observe> Observe for [T] {
    type Observer<'ob, S, D>
        = SliceObserver<VecObserverState<T::Observer<'ob, T, Zero>>, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl<T> Unsize for [T] {
    type Slice = Self;

    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    fn range_from(&self, from: usize) -> &Self::Slice {
        &self[from..]
    }

    unsafe fn removed_len(_ptr: *const u8, new_len: usize, old_len: usize) -> usize {
        old_len - new_len
    }
}

impl<T> RefObserve for [T] {
    type Observer<'ob, S, D>
        = UnsizeObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

#[cfg(test)]
mod tests {
    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn index_by_usize() {
        let slice: &mut [u32] = &mut [0, 1, 2];
        let mut ob = slice.__observe();
        assert_eq!(ob[2], 2);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
        *ob[2].tracked_mut() = 42;
        assert_eq!(ob[2], 42);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(-1, json!(42))));
    }

    #[test]
    fn get_mut() {
        let slice: &mut [u32] = &mut [0, 1, 2];
        let mut ob = slice.__observe();
        assert_eq!(*ob.get_mut(2).unwrap(), 2);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
        *ob.get_mut(2).unwrap().tracked_mut() = 42;
        assert_eq!(*ob.get_mut(2).unwrap(), 42);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(-1, json!(42))));
    }

    #[test]
    fn swap() {
        let slice: &mut [u32] = &mut [0, 1, 2];
        let mut ob = slice.__observe();
        ob.swap(0, 1);
        assert_eq!(*ob.untracked_ref(), [1, 0, 2]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, replace!(-2, json!(0)), replace!(-3, json!(1))))
        );
    }

    #[test]
    fn boxed_slice_deref_mut_triggers_replace() {
        let mut boxed: Box<[u32]> = vec![1, 2, 3].into_boxed_slice();
        let mut ob = boxed.__observe();
        // Mutate through the slice observer's DerefMut (e.g. via sort).
        ob.sort();
        let Json(mutation) = ob.flush().unwrap();
        // Even though sort is a no-op here (already sorted), DerefMut was triggered
        // so a Replace should be emitted. With diff type `()`, this bug causes None.
        assert!(mutation.is_some(), "DerefMut on Box<[T]> should trigger Replace");
    }
}
