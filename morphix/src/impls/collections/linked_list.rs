//! Observer implementation for [`LinkedList<T>`].

use std::cell::UnsafeCell;
use std::collections::LinkedList;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use serde::Serialize;
use serde::ser::SerializeSeq;

use crate::helper::macros::default_impl_ref_observe;
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{DefaultSpec, Observer, SerializeObserver};
use crate::{MutationKind, Mutations, Observe, PathSegment};

struct LinkedListObserverSideState<O> {
    append_len: usize,
    truncate_len: usize,
    inner: UnsafeCell<LinkedList<O>>,
}

impl<O> LinkedListObserverSideState<O> {
    fn new() -> Self {
        Self {
            append_len: 0,
            truncate_len: 0,
            inner: UnsafeCell::new(LinkedList::new()),
        }
    }
}

struct LinkedListObserverState<O> {
    front: LinkedListObserverSideState<O>,
    back: LinkedListObserverSideState<O>,
}

impl<O> LinkedListObserverState<O> {
    fn mark_replace(&mut self, len: usize) {
        self.front.inner.get_mut().clear();
        self.back.inner.get_mut().clear();
        self.front.append_len = len;
        self.front.truncate_len = len;
        self.back.append_len = 0;
        self.back.truncate_len = 0;
    }
}

impl<O> Invalidate<LinkedList<O::Head>> for LinkedListObserverState<O>
where
    O: Observer<InnerDepth = Zero, Head: Sized>,
{
    fn invalidate(&mut self, list: &LinkedList<O::Head>) {
        self.mark_replace(list.len());
    }
}

/// Observer implementation for [`LinkedList<T>`].
pub struct LinkedListObserver<'ob, O, S: ?Sized, D = Zero> {
    ptr: Pointer<S>,
    state: LinkedListObserverState<O>,
    phantom: PhantomData<&'ob mut D>,
}

impl<'ob, O, S: ?Sized, D> Deref for LinkedListObserver<'ob, O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<'ob, O, S: ?Sized, D> DerefMut for LinkedListObserver<'ob, O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.ptr);
        &mut self.ptr
    }
}

impl<'ob, O, S: ?Sized, D> QuasiObserver for LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDeref<D, Target = LinkedList<O::Head>>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        let len = (*this).untracked_ref().len();
        this.state.mark_replace(len);
    }
}

impl<'ob, O, S: ?Sized, D> Observer for LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = LinkedList<O::Head>>,
{
    fn observe(head: &mut Self::Head) -> Self {
        Self {
            state: LinkedListObserverState {
                front: LinkedListObserverSideState::new(),
                back: LinkedListObserverSideState::new(),
            },
            ptr: Pointer::new(head),
            phantom: PhantomData,
        }
    }

    unsafe fn relocate(this: &mut Self, head: &mut Self::Head) {
        Pointer::set(this, head);
    }
}

struct AppendTail<T> {
    list: *const LinkedList<T>,
    skip: usize,
}

impl<T: Serialize> Serialize for AppendTail<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let list = unsafe { &*self.list };
        let count = list.len() - self.skip;
        let mut seq = serializer.serialize_seq(Some(count))?;
        for item in list.iter().skip(self.skip) {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

impl<'ob, O, S: ?Sized, D> SerializeObserver for LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized> + SerializeObserver,
    O::Head: Serialize + 'static,
    S: AsDerefMut<D, Target = LinkedList<O::Head>>,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        let list = (*this.ptr).as_deref_mut();
        let len = list.len();
        let front_append = core::mem::replace(&mut this.state.front.append_len, 0);
        let front_truncate = core::mem::replace(&mut this.state.front.truncate_len, 0);
        let back_append = core::mem::replace(&mut this.state.back.append_len, 0);
        let back_truncate = core::mem::replace(&mut this.state.back.truncate_len, 0);

        let bb = len - back_append;

        if front_append != front_truncate
            || front_append > bb
            || cfg!(not(feature = "truncate")) && back_truncate > 0
            || cfg!(not(feature = "append")) && back_append > 0
        {
            this.state.front.inner.get_mut().clear();
            this.state.back.inner.get_mut().clear();
            return Mutations::replace(list);
        }

        let mut mutations = Mutations::new();
        #[cfg(feature = "truncate")]
        if back_truncate > 0 {
            mutations.extend(MutationKind::Truncate(back_truncate));
        }
        #[cfg(feature = "append")]
        if back_append > 0 {
            mutations.extend(Mutations::append_owned(AppendTail {
                list: list as *const _,
                skip: bb,
            }));
        }

        let front_inner = this.state.front.inner.get_mut();
        let back_inner = this.state.back.inner.get_mut();

        // Strip appended observers from each end (outermost = front of inner)
        let front_appended_obs = front_append.min(front_inner.len());
        for _ in 0..front_appended_obs {
            front_inner.pop_front();
        }
        let back_appended_obs = back_append.min(back_inner.len());
        for _ in 0..back_appended_obs {
            back_inner.pop_front();
        }

        // Remaining observers are for existing-region elements.
        // Observers may extend into the other end's appended region — truncate to existing bounds.
        let existing_count = bb - front_append;
        while front_inner.len() + back_inner.len() > existing_count {
            // Prefer trimming from the end that extended further
            if front_inner.len() >= back_inner.len() {
                front_inner.pop_back();
            } else {
                back_inner.pop_back();
            }
        }

        let tracked_count = front_inner.len() + back_inner.len();
        let mut is_replace = tracked_count >= existing_count;

        // Process back_inner: back_inner[j] is at absolute position len - back_append - 1 - j
        //   → neg_idx = back_append + 1 + j
        for (j, ob) in back_inner.iter_mut().enumerate() {
            let mutations_j = unsafe { SerializeObserver::flush(ob) };
            is_replace &= mutations_j.is_replace();
            mutations.insert(PathSegment::Negative(back_append + 1 + j), mutations_j);
        }

        // Process front_inner: front_inner[k] is at absolute position front_append + k
        //   → neg_idx = len - front_append - k
        for (k, ob) in front_inner.iter_mut().enumerate().rev() {
            let mutations_k = unsafe { SerializeObserver::flush(ob) };
            is_replace &= mutations_k.is_replace();
            mutations.insert(PathSegment::Negative(len - front_append - k), mutations_k);
        }

        if is_replace && front_append + tracked_count > 0 {
            return Mutations::replace(list);
        }
        for i in (0..front_append).rev() {
            let value = list.iter().nth(i).unwrap();
            mutations.insert(PathSegment::Negative(len - i), Mutations::replace(value));
        }
        mutations
    }
}

/// Iterator returned by [`LinkedListObserver::iter_mut`].
pub struct IterMut<'a, O: Observer<InnerDepth = Zero, Head: Sized>> {
    front: std::vec::IntoIter<*mut O>,
    back: std::vec::IntoIter<*mut O>,
    gap: std::collections::linked_list::IterMut<'a, O::Head>,
    front_inner: *mut LinkedList<O>,
    back_inner: *mut LinkedList<O>,
    _marker: PhantomData<&'a mut O>,
}

impl<'a, O: Observer<InnerDepth = Zero, Head: Sized>> Iterator for IterMut<'a, O> {
    type Item = &'a mut O;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ptr) = self.front.next() {
            return Some(unsafe { &mut *ptr });
        }
        if let Some(value) = self.gap.next() {
            let ob = O::observe(value);
            let front_inner = unsafe { &mut *self.front_inner };
            front_inner.push_back(ob);
            return front_inner.back_mut();
        }
        if let Some(ptr) = self.back.next() {
            return Some(unsafe { &mut *ptr });
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.front.len() + self.gap.len() + self.back.len();
        (len, Some(len))
    }
}

impl<'a, O: Observer<InnerDepth = Zero, Head: Sized>> DoubleEndedIterator for IterMut<'a, O> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if let Some(ptr) = self.back.next_back() {
            return Some(unsafe { &mut *ptr });
        }
        if let Some(value) = self.gap.next_back() {
            let ob = O::observe(value);
            let back_inner = unsafe { &mut *self.back_inner };
            back_inner.push_back(ob);
            return back_inner.back_mut();
        }
        if let Some(ptr) = self.front.next_back() {
            return Some(unsafe { &mut *ptr });
        }
        None
    }
}

impl<'a, O: Observer<InnerDepth = Zero, Head: Sized>> ExactSizeIterator for IterMut<'a, O> {}

impl<'ob, O, S: ?Sized, D> LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = LinkedList<O::Head>>,
{
    fn push_side(this: &mut LinkedListObserverSideState<O>, value: &mut O::Head) {
        this.append_len += 1;
        let this_inner = this.inner.get_mut();
        if !this_inner.is_empty() {
            this_inner.push_front(O::observe(value));
        }
    }

    #[rustversion::since(1.95)]
    fn push_side_mut<'a>(this: &'a mut LinkedListObserverSideState<O>, value: &mut O::Head) -> &'a mut O {
        this.append_len += 1;
        this.inner.get_mut().push_front_mut(O::observe(value))
    }

    fn pop_side(this: &mut LinkedListObserverSideState<O>, other: &mut LinkedListObserverSideState<O>, len: usize) {
        if this.append_len > 0 {
            this.append_len -= 1;
        } else {
            this.truncate_len += 1;
        }
        let this_inner = this.inner.get_mut();
        if !this_inner.is_empty() {
            this_inner.pop_front();
        } else {
            let other_inner = other.inner.get_mut();
            if other_inner.len() > len {
                other_inner.pop_back();
            }
        }
    }

    fn ensure_appended_coverage(
        side: &mut LinkedListObserverSideState<O>,
        mut iter: impl Iterator<Item = *mut O::Head>,
    ) {
        let inner = side.inner.get_mut();
        let covered = inner.len().min(side.append_len);
        let needed = side.append_len - covered;
        for ptr in iter.by_ref().skip(covered).take(needed) {
            inner.push_back(O::observe(unsafe { &mut *ptr }));
        }
    }

    /// See [`LinkedList::append`].
    pub fn append(&mut self, other: &mut LinkedList<O::Head>) {
        self.state.back.append_len += other.len();
        self.untracked_mut().append(other);
    }

    /// See [`LinkedList::iter_mut`].
    pub fn iter_mut(&mut self) -> IterMut<'_, O> {
        let list = (*self.ptr).as_deref_mut();
        let len = list.len();

        // Ensure all appended elements have observers in their respective ends
        // For front: iterate from front, skip already-covered, create missing
        let front_ptrs: Vec<*mut O::Head> = list
            .iter_mut()
            .take(self.state.front.append_len)
            .map(|v| v as *mut _)
            .collect();
        Self::ensure_appended_coverage(&mut self.state.front, front_ptrs.into_iter());
        // For back: iterate from back, skip already-covered, create missing
        let back_ptrs: Vec<*mut O::Head> = list
            .iter_mut()
            .rev()
            .take(self.state.back.append_len)
            .map(|v| v as *mut _)
            .collect();
        Self::ensure_appended_coverage(&mut self.state.back, back_ptrs.into_iter());

        let front_inner = self.state.front.inner.get_mut();
        let back_inner = self.state.back.inner.get_mut();
        let front_len = front_inner.len();
        let back_len = back_inner.len();

        let front_obs_ptrs: Vec<*mut O> = front_inner.iter_mut().map(|o| o as *mut O).collect();
        let back_obs_ptrs: Vec<*mut O> = back_inner.iter_mut().map(|o| o as *mut O).collect();

        let mut gap = list.iter_mut();
        let gap_len = len - front_len - back_len;
        for _ in 0..front_len {
            gap.next();
        }
        for _ in 0..back_len {
            gap.next_back();
        }
        debug_assert_eq!(gap.len(), gap_len);

        IterMut {
            front: front_obs_ptrs.into_iter(),
            back: back_obs_ptrs.into_iter(),
            gap,
            front_inner: self.state.front.inner.get(),
            back_inner: self.state.back.inner.get(),
            _marker: PhantomData,
        }
    }

    /// See [`LinkedList::clear`].
    pub fn clear(&mut self) {
        let len = (*self).untracked_ref().len();
        if len == 0 {
            return;
        }
        self.untracked_mut().clear();
        let existing = len - self.state.front.append_len - self.state.back.append_len;
        self.state.front.inner.get_mut().clear();
        self.state.back.inner.get_mut().clear();
        self.state.front.truncate_len += existing;
        self.state.front.append_len = 0;
        self.state.back.truncate_len = 0;
        self.state.back.append_len = 0;
    }

    /// See [`LinkedList::front_mut`].
    pub fn front_mut(&mut self) -> Option<&mut O> {
        let list = (*self.ptr).as_deref_mut();
        let len = list.len();
        if len == 0 {
            return None;
        }
        let this = &mut self.state.front;
        let other = &mut self.state.back;
        let this_inner = this.inner.get_mut();
        if !this_inner.is_empty() {
            return this_inner.front_mut();
        }
        let other_inner = other.inner.get_mut();
        if other_inner.len() >= len {
            return other_inner.back_mut();
        }
        let value = list.front_mut().unwrap();
        this_inner.push_front(O::observe(value));
        this_inner.front_mut()
    }

    /// See [`LinkedList::back_mut`].
    pub fn back_mut(&mut self) -> Option<&mut O> {
        let list = (*self.ptr).as_deref_mut();
        let len = list.len();
        if len == 0 {
            return None;
        }
        let this = &mut self.state.back;
        let other = &mut self.state.front;
        let this_inner = this.inner.get_mut();
        if !this_inner.is_empty() {
            return this_inner.front_mut();
        }
        let other_inner = other.inner.get_mut();
        if other_inner.len() >= len {
            return other_inner.back_mut();
        }
        let value = list.back_mut().unwrap();
        this_inner.push_front(O::observe(value));
        this_inner.front_mut()
    }

    /// See [`LinkedList::push_front`].
    pub fn push_front(&mut self, value: O::Head) {
        self.untracked_mut().push_front(value);
        let value = (*self.ptr).as_deref_mut().front_mut().unwrap();
        Self::push_side(&mut self.state.front, value);
    }

    /// See [`LinkedList::push_front_mut`].
    #[rustversion::since(1.95)]
    pub fn push_front_mut(&mut self, value: O::Head) -> &mut O {
        let value = (*self.ptr).as_deref_mut().push_front_mut(value);
        Self::push_side_mut(&mut self.state.front, value)
    }

    /// See [`LinkedList::pop_front`].
    pub fn pop_front(&mut self) -> Option<O::Head> {
        let value = self.untracked_mut().pop_front()?;
        let len = (*self).untracked_ref().len();
        Self::pop_side(&mut self.state.front, &mut self.state.back, len);
        Some(value)
    }

    /// See [`LinkedList::push_back`].
    pub fn push_back(&mut self, value: O::Head) {
        self.untracked_mut().push_back(value);
        let value = (*self.ptr).as_deref_mut().back_mut().unwrap();
        Self::push_side(&mut self.state.back, value);
    }

    /// See [`LinkedList::push_back_mut`].
    #[rustversion::since(1.95)]
    pub fn push_back_mut(&mut self, value: O::Head) -> &mut O {
        let value = (*self.ptr).as_deref_mut().push_back_mut(value);
        Self::push_side_mut(&mut self.state.back, value)
    }

    /// See [`LinkedList::pop_back`].
    pub fn pop_back(&mut self) -> Option<O::Head> {
        let value = self.untracked_mut().pop_back()?;
        let len = (*self).untracked_ref().len();
        Self::pop_side(&mut self.state.back, &mut self.state.front, len);
        Some(value)
    }

    /// See [`LinkedList::split_off`].
    pub fn split_off(&mut self, at: usize) -> LinkedList<O::Head> {
        let len = (*self).untracked_ref().len();
        let bb = len - self.state.back.append_len;
        let split = self.untracked_mut().split_off(at);
        if at >= bb {
            // Splitting within the appended region
            let removed = len - at;
            self.state.back.append_len -= removed;
            let back_inner = self.state.back.inner.get_mut();
            for _ in 0..removed.min(back_inner.len()) {
                back_inner.pop_front();
            }
        } else if at > self.state.front.append_len {
            // Splitting within the existing region
            self.state.back.truncate_len += bb - at;
            self.state.back.append_len = 0;
            self.state.back.inner.get_mut().clear();
            let front_inner = self.state.front.inner.get_mut();
            while front_inner.len() > at {
                front_inner.pop_back();
            }
        } else {
            self.state.mark_replace(at);
        }
        split
    }

    /// See [`LinkedList::extract_if`].
    pub fn extract_if<F>(&mut self, filter: F) -> std::collections::linked_list::ExtractIf<'_, O::Head, F>
    where
        F: FnMut(&mut O::Head) -> bool,
    {
        let new_len = (*self).untracked_ref().len();
        self.state.mark_replace(new_len);
        self.untracked_mut().extract_if(filter)
    }
}

impl<'ob, O, S: ?Sized, D, U> Extend<U> for LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = LinkedList<O::Head>>,
    LinkedList<O::Head>: Extend<U>,
{
    fn extend<I: IntoIterator<Item = U>>(&mut self, other: I) {
        let old_len = (*self).untracked_ref().len();
        self.untracked_mut().extend(other);
        let new_len = (*self).untracked_ref().len();
        self.state.back.append_len += new_len - old_len;
    }
}

impl<'ob, O, S: ?Sized, D> Debug for LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    O::Head: Debug,
    S: AsDeref<D, Target = LinkedList<O::Head>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LinkedListObserver")
            .field(&self.untracked_ref())
            .finish()
    }
}

impl<'ob, O, S: ?Sized, D, U> PartialEq<LinkedList<U>> for LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDeref<D, Target = LinkedList<O::Head>>,
    LinkedList<O::Head>: PartialEq<LinkedList<U>>,
{
    fn eq(&self, other: &LinkedList<U>) -> bool {
        self.untracked_ref().eq(other)
    }
}

impl<'ob, O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<LinkedListObserver<'ob, O2, S2, D2>>
    for LinkedListObserver<'ob, O1, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    O1: Observer<InnerDepth = Zero, Head: Sized>,
    O2: Observer<InnerDepth = Zero, Head: Sized>,
    S1: AsDeref<D1, Target = LinkedList<O1::Head>>,
    S2: AsDeref<D2, Target = LinkedList<O2::Head>>,
    LinkedList<O1::Head>: PartialEq<LinkedList<O2::Head>>,
{
    fn eq(&self, other: &LinkedListObserver<'ob, O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<'ob, O, S: ?Sized, D> Eq for LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    O::Head: Eq,
    S: AsDeref<D, Target = LinkedList<O::Head>>,
{
}

impl<T: Observe> Observe for LinkedList<T> {
    type Observer<'ob, S, D>
        = LinkedListObserver<'ob, T::Observer<'ob, T, Zero>, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ref_observe! {
    impl [T: Observe] RefObserve for LinkedList<T>;
}

#[cfg(test)]
#[cfg(feature = "truncate")]
mod tests {
    use std::collections::LinkedList;

    use morphix_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_change() {
        let mut list = LinkedList::from([1, 2, 3]);
        let mut ob = list.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn push_back_append() {
        let mut list = LinkedList::from([1, 2]);
        let mut ob = list.__observe();
        ob.push_back(3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([3]))));
    }

    #[test]
    fn push_front_pop_front() {
        let mut list = LinkedList::from([1, 2, 3]);
        let mut ob = list.__observe();
        ob.push_front(0);
        ob.pop_front();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn push_front_unbalanced() {
        let mut list = LinkedList::from([1, 2]);
        let mut ob = list.__observe();
        ob.push_front(0);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([0, 1, 2]))));
    }

    #[test]
    fn pop_front_triggers_replace() {
        let mut list = LinkedList::from([1, 2, 3]);
        let mut ob = list.__observe();
        ob.pop_front();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([2, 3]))));
    }

    #[test]
    fn pop_back_truncate() {
        let mut list = LinkedList::from([1, 2, 3]);
        let mut ob = list.__observe();
        ob.pop_back();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 1)));
    }

    #[test]
    fn clear_non_empty() {
        let mut list = LinkedList::from([1, 2, 3]);
        let mut ob = list.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([]))));
    }

    #[test]
    fn clear_empty_no_mutation() {
        let mut list: LinkedList<i32> = LinkedList::new();
        let mut ob = list.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn inner_observer_front() {
        let mut list = LinkedList::from(["hello".to_string(), "world".to_string()]);
        let mut ob = list.__observe();
        ob.front_mut().unwrap().push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(-2, json!("!"))));
    }

    #[test]
    fn inner_observer_back() {
        let mut list = LinkedList::from(["hello".to_string(), "world".to_string()]);
        let mut ob = list.__observe();
        ob.back_mut().unwrap().push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(-1, json!("!"))));
    }

    #[test]
    fn extend_appends() {
        let mut list = LinkedList::from([1]);
        let mut ob = list.__observe();
        ob.extend([2, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([2, 3]))));
    }

    #[test]
    fn split_off_in_appended_region() {
        let mut list = LinkedList::from([1, 2]);
        let mut ob = list.__observe();
        ob.push_back(3);
        ob.push_back(4);
        let split = ob.split_off(3);
        assert_eq!(split, LinkedList::from([4]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([3]))));
    }

    #[test]
    fn split_off_in_existing_region() {
        let mut list = LinkedList::from([1, 2, 3, 4]);
        let mut ob = list.__observe();
        let split = ob.split_off(2);
        assert_eq!(split, LinkedList::from([3, 4]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 2)));
    }

    #[test]
    fn append_other_list() {
        let mut list = LinkedList::from([1, 2]);
        let mut ob = list.__observe();
        let mut other = LinkedList::from([3, 4]);
        ob.append(&mut other);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([3, 4]))));
    }

    #[test]
    fn double_flush() {
        let mut list = LinkedList::from([1, 2]);
        let mut ob = list.__observe();
        ob.push_back(3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([3]))));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn iter_mut_all() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        for inner in ob.iter_mut() {
            inner.push_str("!");
        }
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, append!(-1, json!("!")), append!(-2, json!("!"))))
        );
    }

    #[test]
    fn deref_mut_triggers_replace() {
        let mut list = LinkedList::from([1, 2, 3]);
        let mut ob = list.__observe();
        *ob.tracked_mut() = LinkedList::from([10, 20]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([10, 20]))));
    }

    #[test]
    fn pop_back_then_push_back() {
        let mut list = LinkedList::from([1, 2, 3]);
        let mut ob = list.__observe();
        ob.pop_back();
        ob.push_back(4);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 1), append!(_, json!([4])))));
    }

    #[test]
    fn extract_if_triggers_replace() {
        let mut list = LinkedList::from([1, 2, 3, 4]);
        let mut ob = list.__observe();
        let _: Vec<_> = ob.extract_if(|x| *x % 2 == 0).collect();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1, 3]))));
    }

    #[test]
    fn iter_mut_partial_from_both_ends() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()]);
        let mut ob = list.__observe();
        let mut iter = ob.iter_mut();
        iter.next().unwrap().push_str("1");
        iter.next_back().unwrap().push_str("4");
        drop(iter);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, append!(-1, json!("4")), append!(-4, json!("1"))))
        );
    }

    #[test]
    fn front_and_back_mut_independent() {
        let mut list = LinkedList::from(["x".to_string(), "y".to_string(), "z".to_string()]);
        let mut ob = list.__observe();
        ob.front_mut().unwrap().push_str("1");
        ob.back_mut().unwrap().push_str("3");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, append!(-1, json!("3")), append!(-3, json!("1"))))
        );
    }

    #[rustversion::since(1.95)]
    #[test]
    fn push_back_mut_then_flush() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        ob.push_back_mut("c".to_string()).push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["c!"]))));
    }

    #[rustversion::since(1.95)]
    #[test]
    fn push_back_mut_with_existing_back_observer() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        ob.back_mut().unwrap().push_str("!");
        ob.push_back_mut("c".to_string()).push_str("?");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, append!(_, json!(["c?"])), append!(-2, json!("!"))))
        );
    }

    #[test]
    fn iter_mut_back_then_pop_front() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string(), "c".to_string()]);
        let mut ob = list.__observe();
        let mut iter = ob.iter_mut();
        iter.next_back().unwrap().push_str("!");
        drop(iter);
        ob.pop_front();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(["b", "c!"]))));
    }

    #[test]
    fn iter_mut_back_then_front_mut() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        let mut iter = ob.iter_mut();
        iter.next_back();
        iter.next_back();
        drop(iter);
        ob.front_mut().unwrap().push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(-2, json!("!"))));
    }

    #[test]
    fn push_back_then_back_mut() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        ob.push_back("c".to_string());
        ob.back_mut().unwrap().push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["c!"]))));
    }

    #[test]
    fn push_front_then_front_mut() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        ob.push_front("z".to_string());
        ob.front_mut().unwrap().push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(["z!", "a", "b"]))));
    }

    #[test]
    fn push_back_then_iter_mut_covers_all() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        ob.push_back("c".to_string());
        let count = ob.iter_mut().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn push_front_then_pop_front_cancels() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        ob.push_front("z".to_string());
        ob.pop_front();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn push_back_then_pop_back_cancels() {
        let mut list = LinkedList::from(["a".to_string(), "b".to_string()]);
        let mut ob = list.__observe();
        ob.push_back("c".to_string());
        ob.pop_back();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn multiple_push_back_then_pop_back() {
        let mut list = LinkedList::from([1, 2]);
        let mut ob = list.__observe();
        ob.push_back(3);
        ob.push_back(4);
        ob.pop_back();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([3]))));
    }

    #[test]
    fn multiple_push_front_then_pop_front() {
        let mut list = LinkedList::from([1, 2]);
        let mut ob = list.__observe();
        ob.push_front(0);
        ob.push_front(-1);
        ob.pop_front();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([0, 1, 2]))));
    }

    #[test]
    fn push_back_then_front_mut_all_appended() {
        let mut list: LinkedList<String> = LinkedList::new();
        let mut ob = list.__observe();
        ob.push_back("a".to_string());
        ob.push_back("b".to_string());
        ob.front_mut().unwrap().push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["a!", "b"]))));
    }

    #[test]
    fn push_back_mixed_then_back_mut() {
        let mut list = LinkedList::from(["x".to_string()]);
        let mut ob = list.__observe();
        ob.push_back("a".to_string());
        ob.push_back("b".to_string());
        ob.back_mut().unwrap().push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["a", "b!"]))));
    }

    #[rustversion::since(1.95)]
    #[test]
    fn push_back_maintains_back_inner_symmetry() {
        let mut list = LinkedList::from(["x".to_string()]);
        let mut ob = list.__observe();
        ob.push_back_mut("a".to_string()).push_str("!");
        ob.push_back("b".to_string());
        ob.back_mut().unwrap().push_str("?");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["a!", "b?"]))));
    }
}
