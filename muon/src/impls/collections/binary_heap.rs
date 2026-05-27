//! Observer implementation for [`BinaryHeap<T>`].

use std::collections::binary_heap::{self, Drain};
use std::collections::{BinaryHeap, TryReserveError};
use std::ops::{Deref, DerefMut};

use serde::Serialize;

use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::{default_impl_ro_observe, delegate_methods};
use crate::helper::shallow::shallow_observer;
use crate::helper::{AsDerefMut, QuasiObserver, Unsigned};
use crate::observe::DefaultSpec;
use crate::{MutationKind, Mutations, Observe};

shallow_observer! {
    /// Observer implementation for [`BinaryHeap<T>`].
    struct BinaryHeapObserver(for<T> BinaryHeap<T>);
}

/// Handle produced by [`BinaryHeapObserver::peek_mut`].
pub struct PeekMut<'a, T: Ord> {
    inner: binary_heap::PeekMut<'a, T>,
    state: *mut bool,
}

impl<'a, T: Ord> Deref for PeekMut<'a, T> {
    type Target = binary_heap::PeekMut<'a, T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T: Ord> DerefMut for PeekMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { *self.state = true }
        &mut self.inner
    }
}

impl<'a, T: Ord> PeekMut<'a, T> {
    /// See [`binary_heap::PeekMut::pop`].
    pub fn pop(this: PeekMut<'a, T>) -> T {
        unsafe { *this.state = true }
        binary_heap::PeekMut::pop(this.inner)
    }
}

struct LenGuard<'a, T> {
    old_len: usize,
    state: &'a mut bool,
    inner: &'a mut BinaryHeap<T>,
}

impl<T> Drop for LenGuard<'_, T> {
    fn drop(&mut self) {
        if self.old_len != self.inner.len() {
            *self.state = true;
        }
    }
}

impl<T> Deref for LenGuard<'_, T> {
    type Target = BinaryHeap<T>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<T> DerefMut for LenGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl<'ob, S: ?Sized, D, T> BinaryHeapObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BinaryHeap<T>>,
{
    fn nonempty_mut(&mut self) -> &mut BinaryHeap<T> {
        if (*self).untracked_ref().is_empty() {
            self.untracked_mut()
        } else {
            self.tracked_mut()
        }
    }

    fn guarded_mut(&mut self) -> LenGuard<'_, T> {
        let inner = (*self.ptr).as_deref_mut();
        LenGuard {
            old_len: inner.len(),
            state: &mut self.state,
            inner,
        }
    }

    delegate_methods! { untracked_mut() as BinaryHeap =>
        pub fn reserve_exact(&mut self, additional: usize);
        pub fn reserve(&mut self, additional: usize);
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
    }

    delegate_methods! { nonempty_mut() as BinaryHeap =>
        pub fn drain(&mut self) -> Drain<'_, T>;
        pub fn clear(&mut self);
    }
}

impl<'ob, S: ?Sized, D, T> BinaryHeapObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BinaryHeap<T>>,
    T: Ord,
{
    /// See [`BinaryHeap::peek_mut`].
    pub fn peek_mut(&mut self) -> Option<PeekMut<'_, T>> {
        let inner = (*self.ptr).as_deref_mut().peek_mut()?;
        Some(PeekMut {
            inner,
            state: &raw mut self.state,
        })
    }

    delegate_methods! { tracked_mut() as BinaryHeap =>
        pub fn push(&mut self, item: T);
    }

    delegate_methods! { guarded_mut() as BinaryHeap =>
        pub fn pop(&mut self) -> Option<T>;
        pub fn append(&mut self, other: &mut BinaryHeap<T>);
        pub fn retain<F>(&mut self, f: F) where F: FnMut(&T) -> bool;
    }
}

impl<'ob, S: ?Sized, D, T, U> Extend<U> for BinaryHeapObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = BinaryHeap<T>>,
    BinaryHeap<T>: Extend<U>,
{
    fn extend<I: IntoIterator<Item = U>>(&mut self, iter: I) {
        self.guarded_mut().extend(iter);
    }
}

impl<T> Observe for BinaryHeap<T> {
    type Observer<'ob, S, D>
        = BinaryHeapObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ro_observe! {
    impl [T] RoObserve for BinaryHeap<T>;
}

impl<T: Serialize + Clone + Ord> Snapshot for BinaryHeap<T> {
    type Snapshot = Box<[T]>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.iter().cloned().collect()
    }
}

struct AppendTail<'a, T> {
    heap: &'a BinaryHeap<T>,
    skip: usize,
}

impl<T: Serialize> Serialize for AppendTail<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let count = self.heap.len() - self.skip;
        let mut seq = serializer.serialize_seq(Some(count))?;
        for item in self.heap.iter().skip(self.skip) {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

impl<T: Serialize + Clone + Ord> SerializeSnapshot for BinaryHeap<T> {
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
            mutations.extend(Mutations::append_owned(AppendTail {
                heap: self,
                skip: prefix_len,
            }));
            #[cfg(not(feature = "append"))]
            return Mutations::replace(self);
        }
        mutations
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BinaryHeap;

    use serde_json::Value;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};
    use crate::{Mutation, MutationKind};

    fn is_replace(mutation: &Option<Mutation<Value>>) -> bool {
        match mutation {
            Some(m) => m.path.is_empty() && matches!(m.kind, MutationKind::Replace(_)),
            None => false,
        }
    }

    #[test]
    fn no_change() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn push_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2]);
        let mut ob = heap.__observe();
        ob.push(3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn pop_non_empty_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        assert_eq!(ob.pop(), Some(3));
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn pop_empty_no_mutation() {
        let mut heap: BinaryHeap<i32> = BinaryHeap::new();
        let mut ob = heap.__observe();
        assert_eq!(ob.pop(), None);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn peek_mut_empty_returns_none() {
        let mut heap: BinaryHeap<i32> = BinaryHeap::new();
        let mut ob = heap.__observe();
        assert!(ob.peek_mut().is_none());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn peek_mut_read_only_no_mutation() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        {
            let peeked = ob.peek_mut().unwrap();
            assert_eq!(**peeked, 3);
        }
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn peek_mut_deref_mut_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        {
            let mut peeked = ob.peek_mut().unwrap();
            **peeked = 10;
        }
        assert_eq!(ob.untracked_ref().peek(), Some(&10));
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn peek_mut_pop_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        let peeked = ob.peek_mut().unwrap();
        assert_eq!(super::PeekMut::pop(peeked), 3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn clear_empty_no_mutation() {
        let mut heap: BinaryHeap<i32> = BinaryHeap::new();
        let mut ob = heap.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn clear_non_empty_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn drain_empty_no_mutation() {
        let mut heap: BinaryHeap<i32> = BinaryHeap::new();
        let mut ob = heap.__observe();
        let drained: Vec<_> = ob.drain().collect();
        assert!(drained.is_empty());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn drain_non_empty_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        let _: Vec<_> = ob.drain().collect();
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn append_empty_other_no_mutation() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut other: BinaryHeap<i32> = BinaryHeap::new();
        let mut ob = heap.__observe();
        ob.append(&mut other);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn append_non_empty_other_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2]);
        let mut other = BinaryHeap::from([3, 4]);
        let mut ob = heap.__observe();
        ob.append(&mut other);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn retain_all_no_mutation() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        ob.retain(|_| true);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn retain_removes_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2, 3, 4]);
        let mut ob = heap.__observe();
        ob.retain(|&x| x % 2 == 0);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn extend_triggers_replace() {
        let mut heap = BinaryHeap::from([1]);
        let mut ob = heap.__observe();
        ob.extend([2, 3, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn extend_empty_iter_no_mutation() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        ob.extend(std::iter::empty::<i32>());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn reserve_no_mutation() {
        let mut heap = BinaryHeap::from([1, 2, 3]);
        let mut ob = heap.__observe();
        ob.reserve(100);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn deref_mut_triggers_replace() {
        let mut heap = BinaryHeap::from([1, 2]);
        let mut ob = heap.__observe();
        *ob.tracked_mut() = BinaryHeap::from([10, 20, 30]);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn double_flush() {
        let mut heap = BinaryHeap::from([1, 2]);
        let mut ob = heap.__observe();
        ob.push(3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }
}
