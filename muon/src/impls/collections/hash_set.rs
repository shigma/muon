//! Observer implementation for [`HashSet<T>`].

use std::borrow::Borrow;
use std::collections::hash_set::Drain;
use std::collections::{HashSet, TryReserveError};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

use serde::Serialize;

use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::{default_impl_ro_observe, delegate_methods};
use crate::helper::shallow::shallow_observer;
use crate::helper::{AsDerefMut, QuasiObserver, Unsigned};
use crate::observe::DefaultSpec;
use crate::{MutationKind, Mutations, Observe};

shallow_observer! {
    /// Observer implementation for [`HashSet<T>`].
    struct HashSetObserver(use<T> HashSet<T>);
}

struct LenGuard<'a, T> {
    old_len: usize,
    state: &'a mut bool,
    inner: &'a mut HashSet<T>,
}

impl<T> Drop for LenGuard<'_, T> {
    fn drop(&mut self) {
        if self.old_len != self.inner.len() {
            *self.state = true;
        }
    }
}

impl<T> Deref for LenGuard<'_, T> {
    type Target = HashSet<T>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<T> DerefMut for LenGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl<'ob, S: ?Sized, D, T> HashSetObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = HashSet<T>>,
{
    fn nonempty_mut(&mut self) -> &mut HashSet<T> {
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

    delegate_methods! { nonempty_mut() as HashSet =>
        pub fn drain(&mut self) -> Drain<'_, T>;
        pub fn clear(&mut self);
    }

    delegate_methods! { guarded_mut() as HashSet =>
        pub fn retain<F>(&mut self, f: F) where F: FnMut(&T) -> bool;
    }

    /// See [`HashSet::extract_if`].
    pub fn extract_if<F>(&mut self, pred: F) -> ExtractIf<'_, T, F>
    where
        F: FnMut(&T) -> bool,
    {
        ExtractIf {
            inner: (*self.ptr).as_deref_mut().extract_if(pred),
            state: &mut self.state,
        }
    }
}

impl<'ob, S: ?Sized, D, T> HashSetObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = HashSet<T>>,
    T: Eq + Hash,
{
    delegate_methods! { untracked_mut() as HashSet =>
        pub fn reserve(&mut self, additional: usize);
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
    }

    delegate_methods! { tracked_mut() as HashSet =>
        pub fn replace(&mut self, value: T) -> Option<T>;
    }

    delegate_methods! { guarded_mut() as HashSet =>
        pub fn insert(&mut self, value: T) -> bool;
        pub fn remove<Q>(&mut self, value: &Q) -> bool where T: Borrow<Q>, Q: Hash + Eq + ?Sized;
        pub fn take<Q>(&mut self, value: &Q) -> Option<T> where T: Borrow<Q>, Q: Hash + Eq + ?Sized;
    }
}

impl<'ob, S: ?Sized, D, T, U> Extend<U> for HashSetObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = HashSet<T>>,
    HashSet<T>: Extend<U>,
{
    fn extend<I: IntoIterator<Item = U>>(&mut self, iter: I) {
        self.guarded_mut().extend(iter)
    }
}

/// Iterator produced by [`HashSetObserver::extract_if`].
pub struct ExtractIf<'a, T, F> {
    inner: std::collections::hash_set::ExtractIf<'a, T, F>,
    state: &'a mut bool,
}

impl<T, F: FnMut(&T) -> bool> Iterator for ExtractIf<'_, T, F> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        let result = self.inner.next();
        if result.is_some() {
            *self.state = true;
        }
        result
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> Observe for HashSet<T> {
    type Observer<'ob, S, D>
        = HashSetObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ro_observe! {
    impl [T] RoObserve for HashSet<T>;
}

impl<T: Serialize + Clone + Eq + Hash> Snapshot for HashSet<T> {
    type Snapshot = Box<[T]>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.iter().cloned().collect()
    }
}

struct AppendTail<'a, T> {
    set: &'a HashSet<T>,
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

impl<T: Serialize + Clone + Eq + Hash> SerializeSnapshot for HashSet<T> {
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
                set: self,
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
    use std::collections::HashSet;

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
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn insert_triggers_replace() {
        let mut set = HashSet::from([1, 2]);
        let mut ob = set.__observe();
        ob.insert(3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn insert_duplicate_no_mutation() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert!(!ob.insert(2));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn remove_existing_triggers_replace() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert!(ob.remove(&2));
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn remove_nonexistent_no_mutation() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert!(!ob.remove(&99));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn take_triggers_replace() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert_eq!(ob.take(&2), Some(2));
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn take_nonexistent_no_mutation() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        assert_eq!(ob.take(&99), None);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn clear_empty_no_mutation() {
        let mut set: HashSet<i32> = HashSet::new();
        let mut ob = set.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn clear_non_empty_triggers_replace() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn double_flush() {
        let mut set = HashSet::from([1, 2]);
        let mut ob = set.__observe();
        ob.insert(3);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn reserve_no_mutation() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.reserve(100);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn extend_triggers_replace() {
        let mut set = HashSet::from([1]);
        let mut ob = set.__observe();
        ob.extend([2, 3, 4]);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn extend_duplicates_no_mutation() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.extend([1, 2, 3]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn deref_mut_triggers_replace() {
        let mut set = HashSet::from([1, 2]);
        let mut ob = set.__observe();
        *ob.tracked_mut() = HashSet::from([10, 20, 30]);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn retain_triggers_replace() {
        let mut set = HashSet::from([1, 2, 3, 4]);
        let mut ob = set.__observe();
        ob.retain(|&x| x % 2 == 0);
        assert_eq!(*ob.untracked_ref(), HashSet::from([2, 4]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn retain_all_no_mutation() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.retain(|_| true);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn extract_if_triggers_replace() {
        let mut set = HashSet::from([1, 2, 3, 4]);
        let mut ob = set.__observe();
        let extracted: HashSet<_> = ob.extract_if(|&x| x % 2 == 0).collect();
        assert_eq!(extracted, HashSet::from([2, 4]));
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }

    #[test]
    fn extract_if_none_no_mutation() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        let extracted: Vec<_> = ob.extract_if(|_| false).collect();
        assert!(extracted.is_empty());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn extract_if_no_consume_no_mutation() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        let _ = ob.extract_if(|_| true);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn drain_empty_no_mutation() {
        let mut set: HashSet<i32> = HashSet::new();
        let mut ob = set.__observe();
        let drained: Vec<_> = ob.drain().collect();
        assert!(drained.is_empty());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_triggers_replace() {
        let mut set = HashSet::from([1, 2, 3]);
        let mut ob = set.__observe();
        ob.replace(2);
        let Json(mutation) = ob.flush().unwrap();
        assert!(is_replace(&mutation));
    }
}
