//! Observer implementation for [`VecDeque<T>`].

use std::cell::UnsafeCell;
use std::collections::vec_deque::Drain;
use std::collections::{TryReserveError, VecDeque};
use std::fmt::Debug;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};

use serde::Serialize;

use crate::helper::macros::{default_impl_ref_observe, delegate_methods};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{DefaultSpec, Observer, SerializeObserver};
use crate::{MutationKind, Mutations, Observe, PathSegment};

struct VecDequeObserverState<O> {
    front_prepend_len: usize,
    front_truncate_len: usize,
    back_append_len: usize,
    back_truncate_len: usize,
    inner: UnsafeCell<VecDeque<O>>,
}

impl<O> VecDequeObserverState<O> {
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

impl<O> Invalidate<VecDeque<O::Head>> for VecDequeObserverState<O>
where
    O: Observer<InnerDepth = Zero, Head: Sized>,
{
    fn invalidate(&mut self, deque: &VecDeque<O::Head>) {
        self.mark_replace(deque.len());
    }
}

/// Observer implementation for [`VecDeque<T>`].
pub struct VecDequeObserver<'ob, O, S: ?Sized, D = Zero> {
    ptr: Pointer<S>,
    state: VecDequeObserverState<O>,
    phantom: PhantomData<&'ob mut D>,
}

impl<'ob, O, S: ?Sized, D> Deref for VecDequeObserver<'ob, O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<'ob, O, S: ?Sized, D> DerefMut for VecDequeObserver<'ob, O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.ptr);
        &mut self.ptr
    }
}

impl<'ob, O, S: ?Sized, D> QuasiObserver for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDeref<D, Target = VecDeque<O::Head>>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        let len = (*this).untracked_ref().len();
        this.state.mark_replace(len);
    }
}

impl<'ob, O, S: ?Sized, D> Observer for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = VecDeque<O::Head>>,
{
    fn observe(head: &mut Self::Head) -> Self {
        Self {
            state: VecDequeObserverState {
                front_prepend_len: 0,
                front_truncate_len: 0,
                back_append_len: 0,
                back_truncate_len: 0,
                inner: UnsafeCell::new(VecDeque::new()),
            },
            ptr: Pointer::new(head),
            phantom: PhantomData,
        }
    }

    unsafe fn relocate(this: &mut Self, head: &mut Self::Head) {
        Pointer::set(this, head);
    }
}

impl<'ob, O, S: ?Sized, D> SerializeObserver for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized> + SerializeObserver,
    O::Head: Serialize + 'static,
    S: AsDerefMut<D, Target = VecDeque<O::Head>>,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        let deque = (*this.ptr).as_deref_mut();
        let len = deque.len();
        let front_prepend_len = core::mem::replace(&mut this.state.front_prepend_len, 0);
        let front_truncate_len = core::mem::replace(&mut this.state.front_truncate_len, 0);
        let back_append_len = core::mem::replace(&mut this.state.back_append_len, 0);
        let back_truncate_len = core::mem::replace(&mut this.state.back_truncate_len, 0);

        let slice = deque.make_contiguous();
        let back_boundary = len - back_append_len;

        // unbalanced front / feature gate fallback
        if front_prepend_len != front_truncate_len
            || cfg!(not(feature = "truncate")) && back_truncate_len > 0
            || cfg!(not(feature = "append")) && back_append_len > 0
        {
            this.state.inner.get_mut().clear();
            return Mutations::replace(slice);
        }

        // Relocate must precede Mutations::append/replace: relocate takes `&mut slice` (Unique retag),
        // which would invalidate a SerializeRef's SRO tag if created first. Passing `..back_boundary` drops
        // stale inner observers for appended elements via `relocate`'s internal truncate.
        unsafe { relocate(&this.state.inner, &mut slice[..back_boundary]) };

        let mut mutations = Mutations::new();
        #[cfg(feature = "truncate")]
        if back_truncate_len > 0 {
            mutations.extend(MutationKind::Truncate(back_truncate_len));
        }
        #[cfg(feature = "append")]
        if back_append_len > 0 {
            mutations.extend(Mutations::append(&slice[back_boundary..]));
        }

        let prepend_len = front_prepend_len.min(back_boundary);
        let inner = this.state.inner.get_mut().make_contiguous();
        let mut is_replace = true;
        for (index, ob) in inner[prepend_len..].iter_mut().enumerate().rev() {
            let mutations_i = unsafe { SerializeObserver::flush(ob) };
            is_replace &= mutations_i.is_replace();
            mutations.insert(PathSegment::Negative(len - prepend_len - index), mutations_i);
        }
        if is_replace && (prepend_len > 0 || !mutations.is_empty()) {
            return Mutations::replace(slice);
        }
        for i in (0..prepend_len).rev() {
            mutations.insert(PathSegment::Negative(len - i), Mutations::replace(&slice[i]));
        }
        mutations
    }
}

/// Ensures element observers exist for all elements and updates their pointers.
unsafe fn relocate<O>(inner: &UnsafeCell<VecDeque<O>>, deque_slice: &mut [O::Head])
where
    O: Observer<InnerDepth = Zero, Head: Sized>,
{
    let observers = unsafe { &mut *inner.get() };
    if observers.len() < deque_slice.len() {
        for value in deque_slice[observers.len()..].iter_mut() {
            observers.push_back(O::observe(value));
        }
    }
    observers.truncate(deque_slice.len());
    let ob_contiguous = observers.make_contiguous();
    for (ob, value) in ob_contiguous.iter_mut().zip(deque_slice.iter_mut()) {
        unsafe { Observer::relocate(ob, value) };
    }
}

impl<'ob, O, S: ?Sized, D> VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = VecDeque<O::Head>>,
{
    #[expect(clippy::mut_from_ref)]
    fn force_index(&self, index: usize) -> Option<&mut O> {
        let deque = unsafe { Pointer::as_mut(&self.ptr).as_deref_mut() };
        let len = deque.len();
        if index >= len {
            return None;
        }
        let bb = self.state.back_boundary(len);
        let slice = deque.make_contiguous();
        unsafe { relocate(&self.state.inner, &mut slice[..bb]) };
        let observers = unsafe { &mut *self.state.inner.get() };
        observers.get_mut(index)
    }

    fn force_all(&mut self) -> &mut VecDeque<O> {
        let deque = (*self.ptr).as_deref_mut();
        let slice = deque.make_contiguous();
        unsafe { relocate(&self.state.inner, slice) };
        self.state.inner.get_mut()
    }

    /// See [`VecDeque::get_mut`].
    pub fn get_mut(&mut self, index: usize) -> Option<&mut O> {
        let deque = (*self.ptr).as_deref_mut();
        let len = deque.len();
        if index >= len {
            return None;
        }
        let bb = self.state.back_boundary(len);
        let slice = deque.make_contiguous();
        unsafe { relocate(&self.state.inner, &mut slice[..bb]) };
        self.state.inner.get_mut().get_mut(index)
    }

    /// See [`VecDeque::swap`].
    pub fn swap(&mut self, i: usize, j: usize) {
        if i != j {
            let observers = self.state.inner.get_mut();
            if let Some(ob) = observers.get_mut(i) {
                QuasiObserver::invalidate(ob);
            }
            if let Some(ob) = observers.get_mut(j) {
                QuasiObserver::invalidate(ob);
            }
            self.untracked_mut().swap(i, j);
        }
    }

    delegate_methods! { untracked_mut() as VecDeque =>
        pub fn reserve_exact(&mut self, additional: usize);
        pub fn reserve(&mut self, additional: usize);
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
    }

    /// See [`VecDeque::truncate`].
    pub fn truncate(&mut self, len: usize) {
        let old_len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(old_len);
        if len >= bb {
            // Only truncating appended
            self.state.back_append_len -= bb.min(old_len) + len - old_len;
            self.untracked_mut().truncate(len);
        } else if len > self.state.front_prepend_len {
            // Truncating into existing from back
            self.state.back_truncate_len += bb - len;
            self.state.back_append_len = 0;
            let inner = self.state.inner.get_mut();
            if inner.len() > len {
                inner.truncate(len);
            }
            self.untracked_mut().truncate(len);
        } else {
            // All existing gone
            self.untracked_mut().truncate(len);
            self.state.mark_replace(len);
        }
    }

    /// See [`VecDeque::iter_mut`].
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut O> {
        let observers = self.force_all();
        observers.iter_mut()
    }

    /// See [`VecDeque::as_mut_slices`].
    pub fn as_mut_slices(&mut self) -> (&mut [O], &mut [O]) {
        self.force_all();
        self.state.inner.get_mut().as_mut_slices()
    }

    /// See [`VecDeque::range_mut`].
    pub fn range_mut<R>(&mut self, range: R) -> impl Iterator<Item = &mut O>
    where
        R: RangeBounds<usize> + Clone,
    {
        let deque = (*self.ptr).as_deref_mut();
        let len = deque.len();
        let bb = self.state.back_boundary(len);
        let slice = deque.make_contiguous();
        unsafe { relocate(&self.state.inner, &mut slice[..bb]) };
        self.state.inner.get_mut().range_mut(range)
    }

    /// See [`VecDeque::drain`].
    pub fn drain<R>(&mut self, range: R) -> Drain<'_, O::Head>
    where
        R: RangeBounds<usize>,
    {
        let old_len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(old_len);
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded => old_len,
        };
        if start >= bb {
            self.state.back_append_len -= end - start;
            return self.untracked_mut().drain(range);
        }
        let new_len = old_len - (end - start);
        self.state.mark_replace(new_len);
        self.untracked_mut().drain(range)
    }

    /// See [`VecDeque::clear`].
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

    /// See [`VecDeque::front_mut`].
    pub fn front_mut(&mut self) -> Option<&mut O> {
        if (*self).untracked_ref().is_empty() {
            return None;
        }
        self.get_mut(0)
    }

    /// See [`VecDeque::back_mut`].
    pub fn back_mut(&mut self) -> Option<&mut O> {
        let len = (*self).untracked_ref().len();
        if len == 0 {
            return None;
        }
        self.get_mut(len - 1)
    }

    /// See [`VecDeque::pop_front`].
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

    /// See [`VecDeque::pop_back`].
    pub fn pop_back(&mut self) -> Option<O::Head> {
        let value = self.untracked_mut().pop_back()?;
        if self.state.back_append_len > 0 {
            self.state.back_append_len -= 1;
        } else {
            self.state.back_truncate_len += 1;
            let len = (*self).untracked_ref().len();
            let bb = self.state.back_boundary(len);
            let inner = self.state.inner.get_mut();
            if inner.len() > bb {
                inner.truncate(bb);
            }
        }
        Some(value)
    }

    /// See [`VecDeque::pop_front_if`].
    #[rustversion::since(1.93)]
    pub fn pop_front_if(&mut self, predicate: impl FnOnce(&mut O::Head) -> bool) -> Option<O::Head> {
        let front = self.untracked_mut().front_mut()?;
        if predicate(front) { self.pop_front() } else { None }
    }

    /// See [`VecDeque::pop_back_if`].
    #[rustversion::since(1.93)]
    pub fn pop_back_if(&mut self, predicate: impl FnOnce(&mut O::Head) -> bool) -> Option<O::Head> {
        let back = self.untracked_mut().back_mut()?;
        if predicate(back) { self.pop_back() } else { None }
    }

    /// See [`VecDeque::push_front`].
    pub fn push_front(&mut self, value: O::Head) {
        self.state.front_prepend_len += 1;
        self.untracked_mut().push_front(value);
        let inner = self.state.inner.get_mut();
        if !inner.is_empty() {
            let deque = (*self.ptr).as_deref_mut();
            let slice = deque.make_contiguous();
            inner.push_front(O::observe(&mut slice[0]));
        }
    }

    /// See [`VecDeque::push_back`].
    pub fn push_back(&mut self, value: O::Head) {
        self.state.back_append_len += 1;
        self.untracked_mut().push_back(value);
    }

    /// See [`VecDeque::push_front_mut`].
    #[rustversion::since(1.95)]
    pub fn push_front_mut(&mut self, value: O::Head) -> &mut O {
        self.push_front(value);
        self.force_all().front_mut().unwrap()
    }

    /// See [`VecDeque::push_back_mut`].
    #[rustversion::since(1.95)]
    pub fn push_back_mut(&mut self, value: O::Head) -> &mut O {
        self.state.back_append_len += 1;
        self.untracked_mut().push_back(value);
        self.force_all().back_mut().unwrap()
    }

    /// See [`VecDeque::swap_remove_front`].
    pub fn swap_remove_front(&mut self, index: usize) -> Option<O::Head> {
        let len = (*self).untracked_ref().len();
        let fc = self.state.front_prepend_len;
        let value = self.untracked_mut().swap_remove_front(index)?;
        if index < fc {
            self.state.front_prepend_len -= 1;
        } else if index == fc {
            self.state.front_truncate_len += 1;
        } else {
            self.state.mark_replace(len - 1);
            return Some(value);
        }
        let inner = self.state.inner.get_mut();
        if !inner.is_empty() {
            inner.pop_front();
        }
        Some(value)
    }

    /// See [`VecDeque::swap_remove_back`].
    pub fn swap_remove_back(&mut self, index: usize) -> Option<O::Head> {
        let len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(len);
        let value = self.untracked_mut().swap_remove_back(index)?;
        if index >= bb {
            self.state.back_append_len -= 1;
        } else if index + 1 == bb && self.state.back_append_len == 0 {
            self.state.back_truncate_len += 1;
            let inner = self.state.inner.get_mut();
            if inner.len() > index {
                inner.truncate(index);
            }
        } else {
            self.state.mark_replace(len - 1);
        }
        Some(value)
    }

    /// See [`VecDeque::insert`].
    pub fn insert(&mut self, index: usize, value: O::Head) {
        let len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(len);
        if index >= bb {
            self.state.back_append_len += 1;
            self.untracked_mut().insert(index, value);
        } else if index <= self.state.front_prepend_len {
            self.state.front_prepend_len += 1;
            self.untracked_mut().insert(index, value);
            let inner = self.state.inner.get_mut();
            if !inner.is_empty() {
                let deque = (*self.ptr).as_deref_mut();
                let slice = deque.make_contiguous();
                inner.push_front(O::observe(&mut slice[index]));
            }
        } else {
            self.untracked_mut().insert(index, value);
            self.state.mark_replace(len + 1);
        }
    }

    /// See [`VecDeque::insert_mut`].
    #[rustversion::since(1.95)]
    pub fn insert_mut(&mut self, index: usize, value: O::Head) -> &mut O {
        self.insert(index, value);
        self.force_all().get_mut(index).unwrap()
    }

    /// See [`VecDeque::remove`].
    pub fn remove(&mut self, index: usize) -> Option<O::Head> {
        let len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(len);
        let value = self.untracked_mut().remove(index)?;
        if index >= bb {
            self.state.back_append_len -= 1;
        } else if index + 1 == bb {
            self.state.back_truncate_len += 1;
            self.state.back_append_len = 0;
            let inner = self.state.inner.get_mut();
            if inner.len() > index {
                inner.truncate(index);
            }
        } else {
            self.state.mark_replace(len - 1);
        }
        Some(value)
    }

    /// See [`VecDeque::split_off`].
    pub fn split_off(&mut self, at: usize) -> VecDeque<O::Head> {
        let len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(len);
        let split = self.untracked_mut().split_off(at);
        if at >= bb {
            self.state.back_append_len -= len - at;
        } else if at > self.state.front_prepend_len {
            self.state.back_truncate_len += bb - at;
            self.state.back_append_len = 0;
            let inner = self.state.inner.get_mut();
            if inner.len() > at {
                inner.truncate(at);
            }
        } else {
            self.state.mark_replace(at);
        }
        split
    }

    /// See [`VecDeque::append`].
    pub fn append(&mut self, other: &mut VecDeque<O::Head>) {
        self.state.back_append_len += other.len();
        self.untracked_mut().append(other);
    }

    /// See [`VecDeque::retain`].
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&O::Head) -> bool,
    {
        self.untracked_mut().retain(f);
        let new_len = (*self).untracked_ref().len();
        self.state.mark_replace(new_len);
    }

    /// See [`VecDeque::retain_mut`].
    pub fn retain_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut O::Head) -> bool,
    {
        self.untracked_mut().retain_mut(f);
        let new_len = (*self).untracked_ref().len();
        self.state.mark_replace(new_len);
    }

    /// See [`VecDeque::resize_with`].
    pub fn resize_with(&mut self, new_len: usize, generator: impl FnMut() -> O::Head) {
        let old_len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(old_len);
        self.untracked_mut().resize_with(new_len, generator);
        if new_len >= bb {
            self.state.back_append_len += new_len - old_len;
        } else if new_len > self.state.front_prepend_len {
            self.state.back_truncate_len += bb - new_len;
            self.state.back_append_len = 0;
            let inner = self.state.inner.get_mut();
            if inner.len() > new_len {
                inner.truncate(new_len);
            }
        } else {
            self.state.mark_replace(new_len);
        }
    }

    /// See [`VecDeque::make_contiguous`].
    pub fn make_contiguous(&mut self) -> &mut [O] {
        let deque = (*self.ptr).as_deref_mut();
        let len = deque.len();
        let bb = self.state.back_boundary(len);
        let deque_slice = deque.make_contiguous();
        unsafe { relocate(&self.state.inner, &mut deque_slice[..bb]) };
        self.state.inner.get_mut().make_contiguous()
    }

    /// See [`VecDeque::rotate_left`].
    pub fn rotate_left(&mut self, n: usize) {
        let len = (*self).untracked_ref().len();
        if n != 0 && len > 1 {
            self.untracked_mut().rotate_left(n);
            self.state.mark_replace(len);
        }
    }

    /// See [`VecDeque::rotate_right`].
    pub fn rotate_right(&mut self, n: usize) {
        let len = (*self).untracked_ref().len();
        if n != 0 && len > 1 {
            self.untracked_mut().rotate_right(n);
            self.state.mark_replace(len);
        }
    }
}

impl<'ob, O, S: ?Sized, D> VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    O::Head: Clone,
    S: AsDerefMut<D, Target = VecDeque<O::Head>>,
{
    /// See [`VecDeque::resize`].
    pub fn resize(&mut self, new_len: usize, value: O::Head) {
        let old_len = (*self).untracked_ref().len();
        let bb = self.state.back_boundary(old_len);
        self.untracked_mut().resize(new_len, value);
        if new_len >= bb {
            self.state.back_append_len += new_len - old_len;
        } else if new_len > self.state.front_prepend_len {
            self.state.back_truncate_len += bb - new_len;
            self.state.back_append_len = 0;
            let inner = self.state.inner.get_mut();
            if inner.len() > new_len {
                inner.truncate(new_len);
            }
        } else {
            self.state.mark_replace(new_len);
        }
    }
}

impl<'ob, O, S: ?Sized, D, U> Extend<U> for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = VecDeque<O::Head>>,
    VecDeque<O::Head>: Extend<U>,
{
    fn extend<I: IntoIterator<Item = U>>(&mut self, other: I) {
        let old_len = (*self).untracked_ref().len();
        self.untracked_mut().extend(other);
        let new_len = (*self).untracked_ref().len();
        self.state.back_append_len += new_len - old_len;
    }
}

impl<'ob, O, S: ?Sized, D> Read for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head = u8>,
    S: AsDerefMut<D, Target = VecDeque<u8>>,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.untracked_mut().read(buf)?;
        if n > 0 {
            if self.state.front_prepend_len >= n {
                self.state.front_prepend_len -= n;
            } else {
                let from_existing = n - self.state.front_prepend_len;
                self.state.front_truncate_len += from_existing;
                self.state.front_prepend_len = 0;
            }
            let inner = self.state.inner.get_mut();
            for _ in 0..n.min(inner.len()) {
                inner.pop_front();
            }
        }
        Ok(n)
    }
}

impl<'ob, O, S: ?Sized, D> Write for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head = u8>,
    S: AsDerefMut<D, Target = VecDeque<u8>>,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.untracked_mut().write(buf)?;
        self.state.back_append_len += n;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'ob, O, S: ?Sized, D> Index<usize> for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = VecDeque<O::Head>>,
{
    type Output = O;

    fn index(&self, index: usize) -> &Self::Output {
        self.force_index(index).expect("index out of bounds")
    }
}

impl<'ob, O, S: ?Sized, D> IndexMut<usize> for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDerefMut<D, Target = VecDeque<O::Head>>,
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("index out of bounds")
    }
}

impl<'ob, O, S: ?Sized, D> Debug for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    O::Head: Debug,
    S: AsDeref<D, Target = VecDeque<O::Head>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("VecDequeObserver").field(&self.untracked_ref()).finish()
    }
}

impl<'ob, O, S: ?Sized, D, U> PartialEq<VecDeque<U>> for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    S: AsDeref<D, Target = VecDeque<O::Head>>,
    VecDeque<O::Head>: PartialEq<VecDeque<U>>,
{
    fn eq(&self, other: &VecDeque<U>) -> bool {
        self.untracked_ref().eq(other)
    }
}

impl<'ob, O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<VecDequeObserver<'ob, O2, S2, D2>>
    for VecDequeObserver<'ob, O1, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    O1: Observer<InnerDepth = Zero, Head: Sized>,
    O2: Observer<InnerDepth = Zero, Head: Sized>,
    S1: AsDeref<D1, Target = VecDeque<O1::Head>>,
    S2: AsDeref<D2, Target = VecDeque<O2::Head>>,
    VecDeque<O1::Head>: PartialEq<VecDeque<O2::Head>>,
{
    fn eq(&self, other: &VecDequeObserver<'ob, O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<'ob, O, S: ?Sized, D> Eq for VecDequeObserver<'ob, O, S, D>
where
    D: Unsigned,
    O: Observer<InnerDepth = Zero, Head: Sized>,
    O::Head: Eq,
    S: AsDeref<D, Target = VecDeque<O::Head>>,
{
}

impl<T: Observe> Observe for VecDeque<T> {
    type Observer<'ob, S, D>
        = VecDequeObserver<'ob, T::Observer<'ob, T, Zero>, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ref_observe! {
    impl [T: Observe] RefObserve for VecDeque<T>;
}

#[cfg(test)]
#[cfg(feature = "truncate")]
mod tests {
    use std::collections::VecDeque;

    use morphix_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_change_returns_none() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn reserve_returns_none() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.reserve(100);
        ob.shrink_to_fit();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn make_contiguous_returns_none() {
        let mut deque = VecDeque::new();
        deque.push_back(1);
        deque.push_back(2);
        deque.push_front(0);
        let mut ob = deque.__observe();
        ob.make_contiguous();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn push_back_triggers_append() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        ob.push_back(2);
        ob.push_back(3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([2, 3]))));
    }

    #[test]
    fn extend_triggers_append() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        ob.extend([2, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([2, 3]))));
    }

    #[test]
    fn append_other_deque() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        let mut other = VecDeque::from([4, 5]);
        ob.append(&mut other);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([4, 5]))));
    }

    #[test]
    fn pop_back_triggers_truncate() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.pop_back();
        ob.pop_back();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 2)));
    }

    #[test]
    fn truncate_method() {
        let mut deque = VecDeque::from([1, 2, 3, 4, 5]);
        let mut ob = deque.__observe();
        ob.truncate(2);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 3)));
    }

    #[test]
    fn pop_back_then_push_back() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.pop_back();
        ob.push_back(4);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 1), append!(_, json!([4])))));
    }

    #[test]
    fn pop_back_from_appended_region() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        ob.push_back(2);
        ob.push_back(3);
        ob.pop_back();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([2]))));
    }

    #[test]
    fn push_front_triggers_replace() {
        let mut deque = VecDeque::from([1, 2]);
        let mut ob = deque.__observe();
        ob.push_front(0);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([0, 1, 2]))));
    }

    #[test]
    fn pop_front_triggers_replace() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.pop_front();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([2, 3]))));
    }

    #[rustversion::since(1.93)]
    #[test]
    fn pop_front_if_true_triggers_replace() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        let result = ob.pop_front_if(|x| *x == 1);
        assert_eq!(result, Some(1));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([2, 3]))));
    }

    #[rustversion::since(1.93)]
    #[test]
    fn pop_front_if_false_returns_none() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        let result = ob.pop_front_if(|x| *x == 99);
        assert_eq!(result, None);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn push_front_overrides_back_append() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        ob.push_back(2);
        ob.push_front(0);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([0, 1, 2]))));
    }

    #[test]
    fn deref_mut_triggers_replace() {
        let mut deque = VecDeque::from([1, 2]);
        let mut ob = deque.__observe();
        ob.retain(|x| *x > 1);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([2]))));
    }

    #[test]
    fn empty_deque_no_mutation() {
        let mut deque: VecDeque<i32> = VecDeque::new();
        let mut ob = deque.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn empty_deque_push_back() {
        let mut deque: VecDeque<i32> = VecDeque::new();
        let mut ob = deque.__observe();
        ob.push_back(1);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1]))));
    }

    #[test]
    fn empty_deque_push_front() {
        let mut deque: VecDeque<i32> = VecDeque::new();
        let mut ob = deque.__observe();
        ob.push_front(1);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1]))));
    }

    #[test]
    fn clear_empty_deque() {
        let mut deque: VecDeque<i32> = VecDeque::new();
        let mut ob = deque.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn clear_nonempty_deque() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([]))));
    }

    #[test]
    fn split_off_from_existing() {
        let mut deque = VecDeque::from([1, 2, 3, 4]);
        let mut ob = deque.__observe();
        let split = ob.split_off(2);
        assert_eq!(split, VecDeque::from([3, 4]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 2)));
    }

    #[test]
    fn split_off_from_appended() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        ob.push_back(2);
        ob.push_back(3);
        let split = ob.split_off(2);
        assert_eq!(split, VecDeque::from([3]));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([2]))));
    }

    #[test]
    fn remove_at_back_append_boundary() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        let val = ob.remove(2);
        assert_eq!(val, Some(3));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 1)));
    }

    #[test]
    fn remove_from_middle_triggers_replace() {
        let mut deque = VecDeque::from([1, 2, 3, 4]);
        let mut ob = deque.__observe();
        let val = ob.remove(1);
        assert_eq!(val, Some(2));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1, 3, 4]))));
    }

    #[test]
    fn insert_in_appended_region() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        ob.push_back(2);
        ob.insert(2, 3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([2, 3]))));
    }

    #[test]
    fn insert_in_existing_region() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.insert(1, 99);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1, 99, 2, 3]))));
    }

    #[test]
    fn swap_remove_front_triggers_replace() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        let val = ob.swap_remove_front(1);
        assert_eq!(val, Some(2));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1, 3]))));
    }

    #[test]
    fn multiple_flushes() {
        let mut deque = VecDeque::from([1, 2]);
        let mut ob = deque.__observe();
        ob.push_back(3);
        let Json(m1) = ob.flush().unwrap();
        assert_eq!(m1, Some(append!(_, json!([3]))));
        let Json(m2) = ob.flush().unwrap();
        assert_eq!(m2, None);
        ob.pop_back();
        let Json(m3) = ob.flush().unwrap();
        assert_eq!(m3, Some(truncate!(_, 1)));
    }

    #[test]
    fn resize_shrink() {
        let mut deque = VecDeque::from([1, 2, 3, 4, 5]);
        let mut ob = deque.__observe();
        ob.resize(2, 0);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 3)));
    }

    #[test]
    fn resize_grow() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        ob.resize(3, 0);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([0, 0]))));
    }

    #[rustversion::since(1.93)]
    #[test]
    fn pop_back_if_true() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        let result = ob.pop_back_if(|x| *x == 3);
        assert_eq!(result, Some(3));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 1)));
    }

    #[rustversion::since(1.93)]
    #[test]
    fn pop_back_if_false() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        let result = ob.pop_back_if(|x| *x == 99);
        assert_eq!(result, None);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn drain_from_appended() {
        let mut deque = VecDeque::from([1]);
        let mut ob = deque.__observe();
        ob.push_back(2);
        ob.push_back(3);
        let drained: Vec<_> = ob.drain(1..).collect();
        assert_eq!(drained, vec![2, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn drain_straddles_boundary() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.push_back(4);
        let drained: Vec<_> = ob.drain(1..).collect();
        assert_eq!(drained, vec![2, 3, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([1]))));
    }

    #[test]
    fn index_mut_triggers_replace() {
        let mut deque = VecDeque::from([1i32, 2, 3]);
        let mut ob = deque.__observe();
        *ob[1].tracked_mut() = 99;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(-2, json!(99))));
    }

    #[test]
    fn index_returns_inner_observer() {
        let mut deque = VecDeque::from(["hello".to_string(), "world".to_string()]);
        let mut ob = deque.__observe();
        assert_eq!(ob[0], "hello");
        assert_eq!(ob[1], "world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn modify_element_via_inner_observer() {
        let mut deque = VecDeque::from(["hello".to_string(), "world".to_string()]);
        let mut ob = deque.__observe();
        ob[0].push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(-2, json!("!"))));
    }

    #[test]
    fn modify_multiple_elements_via_inner_observer() {
        let mut deque = VecDeque::from(["a".to_string(), "b".to_string(), "c".to_string()]);
        let mut ob = deque.__observe();
        ob[0].push_str("1");
        ob[2].push_str("3");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, append!(-1, json!("3")), append!(-3, json!("1"))))
        );
    }

    #[test]
    fn get_mut_returns_observer() {
        let mut deque = VecDeque::from(["foo".to_string(), "bar".to_string()]);
        let mut ob = deque.__observe();
        let elem = ob.get_mut(1).unwrap();
        elem.push_str("baz");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(-1, json!("baz"))));
    }

    #[test]
    fn front_mut_returns_observer() {
        let mut deque = VecDeque::from(["first".to_string(), "second".to_string()]);
        let mut ob = deque.__observe();
        let front = ob.front_mut().unwrap();
        front.push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(-2, json!("!"))));
    }

    #[test]
    fn back_mut_returns_observer() {
        let mut deque = VecDeque::from(["first".to_string(), "second".to_string()]);
        let mut ob = deque.__observe();
        let back = ob.back_mut().unwrap();
        back.push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(-1, json!("!"))));
    }

    #[test]
    fn iter_mut_returns_observers() {
        let mut deque = VecDeque::from(["a".to_string(), "b".to_string(), "c".to_string()]);
        let mut ob = deque.__observe();
        for elem in ob.iter_mut() {
            elem.push_str("!");
        }
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(
                _,
                append!(-1, json!("!")),
                append!(-2, json!("!")),
                append!(-3, json!("!"))
            ))
        );
    }

    #[test]
    fn make_contiguous_returns_observer_slice() {
        let mut deque = VecDeque::from(["x".to_string(), "y".to_string()]);
        let mut ob = deque.__observe();
        let slice = ob.make_contiguous();
        slice[0].push_str("1");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(-2, json!("1"))));
    }

    #[test]
    fn modify_then_append() {
        let mut deque = VecDeque::from(["a".to_string()]);
        let mut ob = deque.__observe();
        ob[0].push_str("!");
        ob.push_back("b".to_string());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, append!(_, json!(["b"])), append!(-2, json!("!"))))
        );
    }

    #[test]
    fn no_modify_then_append() {
        let mut deque = VecDeque::from(["a".to_string()]);
        let mut ob = deque.__observe();
        let _ = &ob[0];
        ob.push_back("b".to_string());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["b"]))));
    }

    #[test]
    fn modify_element_then_pop_back() {
        let mut deque = VecDeque::from(["a".to_string(), "b".to_string(), "c".to_string()]);
        let mut ob = deque.__observe();
        ob[0].push_str("!");
        ob.pop_back();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 1), append!(-2, json!("!")))));
    }

    #[test]
    fn index_read_only_no_mutation() {
        let mut deque = VecDeque::from(["hello".to_string(), "world".to_string()]);
        let mut ob = deque.__observe();
        let _val = &ob[0];
        let _val2 = &ob[1];
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn pop_push_clears_stale_observer_state() {
        let mut deque = VecDeque::from(["a".to_string(), "b".to_string(), "ab".to_string()]);
        let mut ob = deque.__observe();
        ob[2].truncate(1);
        ob.pop_back();
        ob.push_back("cd".to_string());
        let Json(mutation) = ob.flush().unwrap();
        assert!(mutation.is_some());
        assert_eq!(ob[2], "cd");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[rustversion::since(1.95)]
    #[test]
    fn push_back_mut_returns_observer() {
        let mut deque = VecDeque::from(["a".to_string()]);
        let mut ob = deque.__observe();
        let pushed = ob.push_back_mut("b".into());
        pushed.push_str("c");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["bc"]))));
    }

    #[rustversion::since(1.95)]
    #[test]
    fn push_front_mut_returns_observer() {
        let mut deque = VecDeque::from(["a".to_string(), "b".into()]);
        let mut ob = deque.__observe();
        let pushed = ob.push_front_mut("x".into());
        pushed.push_str("y");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(["xy", "a", "b"]))));
    }

    #[rustversion::since(1.95)]
    #[test]
    fn insert_mut_at_back_returns_observer() {
        let mut deque = VecDeque::from(["a".to_string(), "b".into()]);
        let mut ob = deque.__observe();
        let inserted = ob.insert_mut(2, "c".into());
        inserted.push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(["c!"]))));
    }

    #[rustversion::since(1.95)]
    #[test]
    fn insert_mut_in_middle_returns_observer() {
        let mut deque = VecDeque::from(["a".to_string(), "b".into(), "c".into()]);
        let mut ob = deque.__observe();
        let inserted = ob.insert_mut(1, "X".into());
        inserted.push_str("Y");
        assert_eq!(
            ob,
            VecDeque::from(["a".to_string(), "XY".into(), "b".into(), "c".into()])
        );
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(["a", "XY", "b", "c"]))));
    }

    // --- New tests for front-side fine-grained tracking ---

    #[test]
    fn push_front_pop_front_cancel() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.push_front(0);
        ob.pop_front();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn pop_front_push_front_element_replace() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.pop_front();
        ob.push_front(99);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(-3, json!(99))));
    }

    #[test]
    fn push_front_pop_front_with_back_append() {
        let mut deque = VecDeque::from([1, 2]);
        let mut ob = deque.__observe();
        ob.push_front(0);
        ob.pop_front();
        ob.push_back(3);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!([3]))));
    }

    #[test]
    fn push_back_pop_back_cancel() {
        let mut deque = VecDeque::from([1, 2, 3]);
        let mut ob = deque.__observe();
        ob.push_back(4);
        ob.pop_back();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn pop_front_push_front_with_existing_modify() {
        let mut deque = VecDeque::from(["a".to_string(), "b".to_string(), "c".to_string()]);
        let mut ob = deque.__observe();
        ob[1].push_str("!");
        ob.pop_front();
        ob.push_front("x".to_string());
        // fc=1, ftl=1 (balanced). Element 0 is prepended "x", elements [1,2) are existing "b!", "c".
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, append!(-2, json!("!")), replace!(-3, json!("x"))))
        );
        // Second flush: no stale state.
        let Json(m2) = ob.flush().unwrap();
        assert_eq!(m2, None);
    }
}

#[cfg(test)]
#[cfg(not(feature = "truncate"))]
mod tests_no_truncate {
    use std::collections::VecDeque;

    use serde_json::json;

    use crate::adapter::Json;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn modify_then_pop_back_no_stale_leak() {
        let mut deque = VecDeque::from(["a".to_string(), "b".to_string()]);
        let mut ob = deque.__observe();
        ob[0].push_str("!");
        ob.pop_back();
        let Json(m1) = ob.flush().unwrap();
        assert_eq!(m1, Some(morphix_test_utils::replace!(_, json!(["a!"]))));
        let Json(m2) = ob.flush().unwrap();
        assert_eq!(m2, None);
    }
}
