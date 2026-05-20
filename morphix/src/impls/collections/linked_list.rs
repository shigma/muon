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

struct LinkedListObserverState<O> {
    front_prepend_len: usize,
    front_truncate_len: usize,
    back_append_len: usize,
    back_truncate_len: usize,
    inner: UnsafeCell<LinkedList<O>>,
}

impl<O> LinkedListObserverState<O> {
    fn back_boundary(&self, len: usize) -> usize {
        len - self.back_append_len
    }

    fn mark_replace(&mut self, len: usize) {
        self.inner.get_mut().clear();
        self.front_prepend_len = len;
        self.front_truncate_len = len;
        self.back_append_len = 0;
        self.back_truncate_len = 0;
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
                front_prepend_len: 0,
                front_truncate_len: 0,
                back_append_len: 0,
                back_truncate_len: 0,
                inner: UnsafeCell::new(LinkedList::new()),
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
        let front_prepend_len = core::mem::replace(&mut this.state.front_prepend_len, 0);
        let front_truncate_len = core::mem::replace(&mut this.state.front_truncate_len, 0);
        let back_append_len = core::mem::replace(&mut this.state.back_append_len, 0);
        let back_truncate_len = core::mem::replace(&mut this.state.back_truncate_len, 0);

        let back_boundary = len - back_append_len;

        if front_prepend_len != front_truncate_len
            || cfg!(not(feature = "truncate")) && back_truncate_len > 0
            || cfg!(not(feature = "append")) && back_append_len > 0
        {
            this.state.inner.get_mut().clear();
            return Mutations::replace(list);
        }

        let mut mutations = Mutations::new();
        #[cfg(feature = "truncate")]
        if back_truncate_len > 0 {
            mutations.extend(MutationKind::Truncate(back_truncate_len));
        }
        #[cfg(feature = "append")]
        if back_append_len > 0 {
            mutations.extend(Mutations::append_owned(AppendTail {
                list: list as *const _,
                skip: back_boundary,
            }));
        }

        let prepend_len = front_prepend_len.min(back_boundary);
        let inner = this.state.inner.get_mut();
        let tracked_count = inner.len().saturating_sub(prepend_len);
        let expected_count = back_boundary - prepend_len;
        let mut is_replace = tracked_count >= expected_count;
        for (index, ob) in inner.iter_mut().skip(prepend_len).enumerate().rev() {
            let mutations_i = unsafe { SerializeObserver::flush(ob) };
            is_replace &= mutations_i.is_replace();
            mutations.insert(PathSegment::Negative(len - prepend_len - index), mutations_i);
        }
        if is_replace && (prepend_len > 0 || !mutations.is_empty()) {
            return Mutations::replace(list);
        }
        for i in (0..prepend_len).rev() {
            let value = list.iter().nth(i).unwrap();
            mutations.insert(PathSegment::Negative(len - i), Mutations::replace(value));
        }
        mutations
    }
}

impl<'ob, O, S: ?Sized, D> LinkedListObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = LinkedList<O::Head>>,
{
    fn force_all(&mut self) -> &mut LinkedList<O> {
        let list = (*self.ptr).as_deref_mut();
        let bb = self.state.back_boundary(list.len());
        let inner = self.state.inner.get_mut();
        if inner.len() < bb {
            for value in list.iter_mut().skip(inner.len()).take(bb - inner.len()) {
                inner.push_back(O::observe(value));
            }
        }
        inner
    }

    /// See [`LinkedList::append`].
    pub fn append(&mut self, other: &mut LinkedList<O::Head>) {
        self.state.back_append_len += other.len();
        self.untracked_mut().append(other);
    }

    /// See [`LinkedList::iter_mut`].
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut O> {
        let observers = self.force_all();
        observers.iter_mut()
    }

    /// See [`LinkedList::clear`].
    pub fn clear(&mut self) {
        let len = (*self).untracked_ref().len();
        if len == 0 {
            return;
        }
        self.untracked_mut().clear();
        let existing = len - self.state.front_prepend_len - self.state.back_append_len;
        self.state.inner.get_mut().clear();
        self.state.front_truncate_len += existing;
        self.state.front_prepend_len = 0;
        self.state.back_truncate_len = 0;
        self.state.back_append_len = 0;
    }

    /// See [`LinkedList::front_mut`].
    pub fn front_mut(&mut self) -> Option<&mut O> {
        if (*self).untracked_ref().is_empty() {
            return None;
        }
        let list = (*self.ptr).as_deref_mut();
        let len = list.len();
        let bb = self.state.back_boundary(len);
        if bb == 0 {
            return None;
        }
        let inner = self.state.inner.get_mut();
        if inner.is_empty() {
            let value = list.front_mut().unwrap();
            inner.push_back(O::observe(value));
        }
        inner.front_mut()
    }

    /// See [`LinkedList::back_mut`].
    pub fn back_mut(&mut self) -> Option<&mut O> {
        let list = (*self.ptr).as_deref_mut();
        let len = list.len();
        if len == 0 {
            return None;
        }
        let bb = self.state.back_boundary(len);
        if bb == 0 {
            return None;
        }
        let inner = self.state.inner.get_mut();
        if inner.len() < bb {
            for value in list.iter_mut().skip(inner.len()).take(bb - inner.len()) {
                inner.push_back(O::observe(value));
            }
        }
        inner.back_mut()
    }

    /// See [`LinkedList::push_front`].
    pub fn push_front(&mut self, value: O::Head) {
        self.state.front_prepend_len += 1;
        self.untracked_mut().push_front(value);
        let inner = self.state.inner.get_mut();
        if !inner.is_empty() {
            let list = (*self.ptr).as_deref_mut();
            let head = list.front_mut().unwrap();
            inner.push_front(O::observe(head));
        }
    }

    /// See [`LinkedList::push_front_mut`].
    #[rustversion::since(1.95)]
    pub fn push_front_mut(&mut self, value: O::Head) -> &mut O {
        self.push_front(value);
        self.force_all().front_mut().unwrap()
    }

    /// See [`LinkedList::pop_front`].
    pub fn pop_front(&mut self) -> Option<O::Head> {
        let value = self.untracked_mut().pop_front()?;
        if self.state.front_prepend_len > 0 {
            self.state.front_prepend_len -= 1;
        } else {
            self.state.front_truncate_len += 1;
        }
        let inner = self.state.inner.get_mut();
        if !inner.is_empty() {
            inner.pop_front();
        }
        Some(value)
    }

    /// See [`LinkedList::push_back`].
    pub fn push_back(&mut self, value: O::Head) {
        self.state.back_append_len += 1;
        self.untracked_mut().push_back(value);
    }

    /// See [`LinkedList::push_back_mut`].
    #[rustversion::since(1.95)]
    pub fn push_back_mut(&mut self, value: O::Head) -> &mut O {
        self.state.back_append_len += 1;
        self.untracked_mut().push_back(value);
        self.force_all().back_mut().unwrap()
    }

    /// See [`LinkedList::pop_back`].
    pub fn pop_back(&mut self) -> Option<O::Head> {
        let value = self.untracked_mut().pop_back()?;
        if self.state.back_append_len > 0 {
            self.state.back_append_len -= 1;
        } else {
            self.state.back_truncate_len += 1;
            let len = (*self).untracked_ref().len();
            let bb = self.state.back_boundary(len);
            let inner = self.state.inner.get_mut();
            while inner.len() > bb {
                inner.pop_back();
            }
        }
        Some(value)
    }

    /// See [`LinkedList::split_off`].
    pub fn split_off(&mut self, at: usize) -> LinkedList<O::Head> {
        let len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(len);
        let split = self.untracked_mut().split_off(at);
        if at >= bb {
            self.state.back_append_len -= len - at;
        } else if at > self.state.front_prepend_len {
            self.state.back_truncate_len += bb - at;
            self.state.back_append_len = 0;
            let inner = self.state.inner.get_mut();
            while inner.len() > at {
                inner.pop_back();
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
        self.state.back_append_len += new_len - old_len;
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
}
