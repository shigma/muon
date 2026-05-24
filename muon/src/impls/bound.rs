use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Bound, Deref, DerefMut};

use serde::Serialize;

use crate::Mutations;
use crate::general::Snapshot;
use crate::helper::macros::{spec_impl_observe, spec_impl_ref_observe};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{Observer, RefObserver, SerializeObserver};

struct BoundObserverState<O> {
    initial: bool,
    mutated: bool,
    inner: Bound<O>,
}

impl<O> Invalidate<Bound<O::Head>> for BoundObserverState<O>
where
    O: QuasiObserver<Head: Sized>,
{
    fn invalidate(&mut self, _value: &Bound<O::Head>) {
        self.mutated = true;
        self.inner = Bound::Unbounded;
    }
}

/// Observer implementation for [`Bound<T>`].
pub struct BoundObserver<O, S: ?Sized, D = Zero> {
    ptr: Pointer<S>,
    state: BoundObserverState<O>,
    phantom: PhantomData<D>,
}

impl<O, S: ?Sized, D> Deref for BoundObserver<O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<O, S: ?Sized, D> DerefMut for BoundObserver<O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.ptr);
        &mut self.ptr
    }
}

impl<O, S: ?Sized, D> QuasiObserver for BoundObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D: Unsigned,
    S: AsDeref<D, Target = Bound<O::Head>>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        Invalidate::invalidate(&mut this.state, (*this.ptr).as_deref());
    }
}

impl<O, S: ?Sized, D> Observer for BoundObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Bound<O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
{
    fn observe(head: &mut Self::Head) -> Self {
        let value = head.as_deref_mut();
        let initial = !matches!(value, Bound::Unbounded);
        let inner = match value {
            Bound::Included(v) => Bound::Included(O::observe(v)),
            Bound::Excluded(v) => Bound::Excluded(O::observe(v)),
            Bound::Unbounded => Bound::Unbounded,
        };
        let this = Self {
            state: BoundObserverState {
                initial,
                mutated: false,
                inner,
            },
            ptr: Pointer::new(head),
            phantom: PhantomData,
        };
        Pointer::register_state::<_, D>(&this.ptr, &this.state);
        this
    }

    unsafe fn relocate(this: &mut Self, head: &mut Self::Head) {
        let value = head.as_deref_mut();
        unsafe {
            match (&mut this.state.inner, value) {
                (Bound::Included(o), Bound::Included(v)) => O::relocate(o, v),
                (Bound::Excluded(o), Bound::Excluded(v)) => O::relocate(o, v),
                (Bound::Unbounded, _) => {}
                _ => panic!("inconsistent state for BoundObserver"),
            }
        }
        Pointer::set(this, head);
    }
}

impl<O, S: ?Sized, D> RefObserver for BoundObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Bound<O::Head>>,
    O: RefObserver<InnerDepth = Zero>,
    O::Head: Sized,
{
    fn observe(head: &Self::Head) -> Self {
        let value = head.as_deref();
        let inner = match value {
            Bound::Included(v) => Bound::Included(O::observe(v)),
            Bound::Excluded(v) => Bound::Excluded(O::observe(v)),
            Bound::Unbounded => Bound::Unbounded,
        };
        let this = Self {
            ptr: Pointer::new(head),
            state: BoundObserverState {
                initial: !matches!(value, Bound::Unbounded),
                mutated: false,
                inner,
            },
            phantom: PhantomData,
        };
        Pointer::register_state::<_, D>(&this.ptr, &this.state);
        this
    }

    unsafe fn relocate(this: &mut Self, head: &Self::Head) {
        Pointer::set(this, head);
        let value = head.as_deref();
        unsafe {
            match (&mut this.state.inner, value) {
                (Bound::Included(o), Bound::Included(v)) => O::relocate(o, v),
                (Bound::Excluded(o), Bound::Excluded(v)) => O::relocate(o, v),
                (Bound::Unbounded, _) => {}
                _ => panic!("inconsistent state for BoundObserver"),
            }
        }
    }
}

impl<O, S: ?Sized, D> SerializeObserver for BoundObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Bound<O::Head>>,
    O: SerializeObserver<InnerDepth = Zero>,
    O::Head: Serialize + Sized + 'static,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        let value = (*this.ptr).as_deref();
        let initial = std::mem::replace(&mut this.state.initial, !matches!(value, Bound::Unbounded));
        let mutated = std::mem::take(&mut this.state.mutated);
        if !mutated {
            let mutations = match &mut this.state.inner {
                Bound::Included(o) => unsafe { SerializeObserver::flush(o).with_prefix("Included") },
                Bound::Excluded(o) => unsafe { SerializeObserver::flush(o).with_prefix("Excluded") },
                Bound::Unbounded => Mutations::new(),
            };
            return mutations;
        }
        this.state.inner = Bound::Unbounded;
        if initial || !matches!(value, Bound::Unbounded) {
            Mutations::replace(value)
        } else {
            Mutations::new()
        }
    }

    unsafe fn flat_flush(this: &mut Self) -> Mutations {
        let value = (*this.ptr).as_deref();
        let initial = std::mem::replace(&mut this.state.initial, !matches!(value, Bound::Unbounded));
        let mutated = std::mem::take(&mut this.state.mutated);
        if !mutated {
            let mutations = match &mut this.state.inner {
                Bound::Included(o) => unsafe { SerializeObserver::flat_flush(o).with_prefix("Included") },
                Bound::Excluded(o) => unsafe { SerializeObserver::flat_flush(o).with_prefix("Excluded") },
                _ => panic!("flat_flush can only be called on structs and maps"),
            };
            return mutations;
        }
        this.state.inner = Bound::Unbounded;
        if initial || !matches!(value, Bound::Unbounded) {
            Mutations::replace(value)
        } else {
            Mutations::new()
        }
    }
}

impl<O, S: ?Sized, D> Debug for BoundObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D: Unsigned,
    S: AsDeref<D, Target = Bound<O::Head>>,
    Bound<O::Head>: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BoundObserver").field(&self.untracked_ref()).finish()
    }
}

impl<O, S: ?Sized, D, U> PartialEq<Bound<U>> for BoundObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D: Unsigned,
    S: AsDeref<D, Target = Bound<O::Head>>,
    Bound<O::Head>: PartialEq<Bound<U>>,
{
    fn eq(&self, other: &Bound<U>) -> bool {
        self.untracked_ref().eq(other)
    }
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<BoundObserver<O2, S2, D2>> for BoundObserver<O1, S1, D1>
where
    O1: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    O2: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1, Target = Bound<O1::Head>>,
    S2: AsDeref<D2, Target = Bound<O2::Head>>,
    Bound<O1::Head>: PartialEq<Bound<O2::Head>>,
{
    fn eq(&self, other: &BoundObserver<O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Eq for BoundObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized + Eq>,
    D: Unsigned,
    S: AsDeref<D, Target = Bound<O::Head>>,
{
}

spec_impl_observe!(BoundObserveImpl, Bound<Self>, Bound<T>, BoundObserver);
spec_impl_ref_observe!(BoundRefObserveImpl, Bound<Self>, Bound<T>, BoundObserver);

impl<T: Snapshot> Snapshot for Bound<T> {
    type Snapshot = Bound<T::Snapshot>;

    fn to_snapshot(&self) -> Self::Snapshot {
        match self {
            Bound::Included(v) => Bound::Included(v.to_snapshot()),
            Bound::Excluded(v) => Bound::Excluded(v.to_snapshot()),
            Bound::Unbounded => Bound::Unbounded,
        }
    }

    fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
        match (self, snapshot) {
            (Bound::Included(v), Bound::Included(s)) => v.eq_snapshot(s),
            (Bound::Excluded(v), Bound::Excluded(s)) => v.eq_snapshot(s),
            (Bound::Unbounded, Bound::Unbounded) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use muon_test_utils::*;
    use serde_json::json;

    use super::*;
    use crate::adapter::Json;
    use crate::general::GeneralObserver;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_change_returns_none() {
        let mut bound: Bound<i32> = Bound::Unbounded;
        let mut ob = bound.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);

        let mut bound: Bound<i32> = Bound::Included(1);
        let mut ob = bound.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);

        let mut bound: Bound<i32> = Bound::Excluded(1);
        let mut ob = bound.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn deref_triggers_replace() {
        let mut bound: Bound<i32> = Bound::Included(42);
        let mut ob = bound.__observe();
        *ob.tracked_mut() = Bound::Unbounded;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("Unbounded"))));

        let mut bound: Bound<i32> = Bound::Unbounded;
        let mut ob = bound.__observe();
        *ob.tracked_mut() = Bound::Included(42);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!({"Included": 42}))));

        let mut bound: Bound<i32> = Bound::Included(1);
        let mut ob = bound.__observe();
        *ob.tracked_mut() = Bound::Excluded(2);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!({"Excluded": 2}))));

        let mut bound: Bound<i32> = Bound::Unbounded;
        let mut ob = bound.__observe();
        *ob.tracked_mut() = Bound::Included(1);
        *ob.tracked_mut() = Bound::Unbounded;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn inner_change_granular() {
        let mut bound: Bound<String> = Bound::Included(String::from("foo"));
        let mut ob = bound.__observe();
        *ob.tracked_mut() = Bound::Included(String::from("bar"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!({"Included": "bar"}))));
    }

    #[test]
    fn specialization() {
        let mut bound: Bound<i32> = Bound::Included(0);
        let ob: GeneralObserver<_, _, _> = bound.__observe();
        assert_eq!(format!("{ob:?}"), "SnapshotObserver(Included(0))");

        let mut bound: Bound<&str> = Bound::Included("");
        let ob: BoundObserver<_, _, _> = bound.__observe();
        assert_eq!(format!("{ob:?}"), r#"BoundObserver(Included(""))"#);
    }

    #[test]
    fn relocate() {
        let mut vec = vec![Bound::Included(String::from("x"))];
        let mut ob = vec.__observe();
        *ob[0].tracked_mut() = Bound::Excluded(String::from("y"));
        ob.reserve(10);
        assert_eq!(*ob[0].untracked_ref(), Bound::Excluded(String::from("y")));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!([{"Excluded": "y"}]))));
    }
}
