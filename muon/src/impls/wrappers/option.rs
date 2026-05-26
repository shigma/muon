use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use serde::Serialize;

use crate::Mutations;
use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::{spec_impl_observe, spec_impl_ro_observe};
use crate::helper::{AsDeref, AsDerefMut, AsDerefPtrExt, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{Observer, SerializeObserver};

struct OptionObserverState<O> {
    initial: bool,
    mutated: bool,
    inner: Option<O>,
}

impl<O> Invalidate<Option<O::Head>> for OptionObserverState<O>
where
    O: QuasiObserver<Head: Sized>,
{
    fn invalidate(&mut self, _value: &Option<O::Head>) {
        self.mutated = true;
        self.inner = None;
    }
}

/// Observer implementation for [`Option<T>`].
pub struct OptionObserver<O, S: ?Sized, D = Zero> {
    ptr: Pointer<S>,
    state: OptionObserverState<O>,
    phantom: PhantomData<D>,
}

impl<O, S: ?Sized, D> Deref for OptionObserver<O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<O, S: ?Sized, D> DerefMut for OptionObserver<O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.ptr);
        &mut self.ptr
    }
}

impl<O, S: ?Sized, D> QuasiObserver for OptionObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D: Unsigned,
    S: AsDeref<D, Target = Option<O::Head>>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        Invalidate::invalidate(&mut this.state, (*this.ptr).as_deref());
    }
}

impl<O, S: ?Sized, D> Observer for OptionObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Option<O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
{
    unsafe fn observe(head: *mut Self::Head) -> Self {
        unsafe {
            let target = head.as_deref_ptr::<D>();
            let value = &*target;
            let this = Self {
                state: OptionObserverState {
                    initial: value.is_some(),
                    mutated: false,
                    inner: value
                        .as_ref()
                        .map(|v| O::observe(target.with_addr(v as *const _ as usize).cast())),
                },
                ptr: Pointer::new_unchecked(head),
                phantom: PhantomData,
            };
            Pointer::register_state::<_, D>(&this.ptr, &this.state);
            this
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe {
            let target = head.as_deref_ptr::<D>();
            match (&mut this.state.inner, &*target) {
                (Some(o), Some(v)) => O::relocate(o, target.with_addr(v as *const _ as usize).cast()),
                (None, _) => {}
                _ => panic!("inconsistent state for OptionObserver"),
            }
            Pointer::set_unchecked(this, head);
        }
    }
}

impl<O, S: ?Sized, D> SerializeObserver for OptionObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Option<O::Head>>,
    O: SerializeObserver<InnerDepth = Zero>,
    O::Head: Serialize + Sized + 'static,
{
    fn flush(this: &mut Self) -> Mutations {
        let option = (*this.ptr).as_deref();
        let initial = std::mem::replace(&mut this.state.initial, option.is_some());
        let mutated = std::mem::take(&mut this.state.mutated);
        if !mutated && initial {
            // Inner must be Some when not mutated and initial was Some.
            return O::flush(this.state.inner.as_mut().unwrap());
        }
        this.state.inner = None;
        if initial || option.is_some() {
            Mutations::replace(option)
        } else {
            Mutations::new()
        }
    }
}

impl<O, S: ?Sized, D> OptionObserver<O, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Option<O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
{
    /// See [`Option::as_mut`].
    pub fn as_mut(&mut self) -> Option<&mut O> {
        let value = (*self.ptr).as_deref_mut().as_mut()?;
        let inner = match &mut self.state.inner {
            Some(inner) => inner,
            slot @ None => slot.insert(unsafe { O::observe(value) }),
        };
        unsafe { O::relocate(inner, value) }
        Some(inner)
    }

    /// See [`Option::insert`].
    pub fn insert(&mut self, value: O::Head) -> &mut O {
        *self.tracked_mut() = Some(value);
        self.as_mut().unwrap()
    }

    /// See [`Option::get_or_insert`].
    pub fn get_or_insert(&mut self, value: O::Head) -> &mut O {
        self.get_or_insert_with(|| value)
    }

    /// See [`Option::get_or_insert_default`].
    pub fn get_or_insert_default(&mut self) -> &mut O
    where
        O::Head: Default,
    {
        self.get_or_insert_with(Default::default)
    }

    /// See [`Option::get_or_insert_with`].
    pub fn get_or_insert_with<F>(&mut self, f: F) -> &mut O
    where
        F: FnOnce() -> O::Head,
    {
        if (*self).untracked_ref().is_none() {
            *self.tracked_mut() = Some(f());
        }
        self.as_mut().unwrap()
    }
}

impl<O, S: ?Sized, D> Debug for OptionObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized + Debug>,
    D: Unsigned,
    S: AsDeref<D, Target = Option<O::Head>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OptionObserver").field(&self.untracked_ref()).finish()
    }
}

impl<O, S: ?Sized, D, U> PartialEq<Option<U>> for OptionObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D: Unsigned,
    S: AsDeref<D, Target = Option<O::Head>>,
    Option<O::Head>: PartialEq<Option<U>>,
{
    fn eq(&self, other: &Option<U>) -> bool {
        self.untracked_ref().eq(other)
    }
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<OptionObserver<O2, S2, D2>> for OptionObserver<O1, S1, D1>
where
    O1: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    O2: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1, Target = Option<O1::Head>>,
    S2: AsDeref<D2, Target = Option<O2::Head>>,
    Option<O1::Head>: PartialEq<Option<O2::Head>>,
{
    fn eq(&self, other: &OptionObserver<O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Eq for OptionObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized + Eq>,
    D: Unsigned,
    S: AsDeref<D, Target = Option<O::Head>>,
{
}

impl<O, S: ?Sized, D, U> PartialOrd<Option<U>> for OptionObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D: Unsigned,
    S: AsDeref<D, Target = Option<O::Head>>,
    Option<O::Head>: PartialOrd<Option<U>>,
{
    fn partial_cmp(&self, other: &Option<U>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other)
    }
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<OptionObserver<O2, S2, D2>> for OptionObserver<O1, S1, D1>
where
    O1: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    O2: QuasiObserver<InnerDepth = Zero, Head: Sized>,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1, Target = Option<O1::Head>>,
    S2: AsDeref<D2, Target = Option<O2::Head>>,
    Option<O1::Head>: PartialOrd<Option<O2::Head>>,
{
    fn partial_cmp(&self, other: &OptionObserver<O2, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Ord for OptionObserver<O, S, D>
where
    O: QuasiObserver<InnerDepth = Zero, Head: Sized + Ord>,
    D: Unsigned,
    S: AsDeref<D, Target = Option<O::Head>>,
{
    fn cmp(&self, other: &OptionObserver<O, S, D>) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

spec_impl_observe!(OptionObserveImpl, Option<Self>, Option<T>, OptionObserver);
spec_impl_ro_observe!(OptionRoObserveImpl, Option<Self>, Option<T>, OptionObserver);

impl<T: Snapshot> Snapshot for Option<T> {
    type Snapshot = Option<T::Snapshot>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.as_ref().map(|v| v.to_snapshot())
    }
}

impl<T: SerializeSnapshot> SerializeSnapshot for Option<T> {
    fn flush(&self, snapshot: Self::Snapshot) -> Mutations {
        match (self, snapshot) {
            (Some(v), Some(s)) => v.flush(s),
            (None, None) => Mutations::new(),
            _ => Mutations::replace(self),
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
        let mut opt: Option<i32> = None;
        let mut ob = opt.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);

        let mut opt: Option<i32> = Some(1);
        let mut ob = opt.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn deref_triggers_replace() {
        let mut opt: Option<i32> = Some(42);
        let mut ob = opt.__observe();
        *ob.tracked_mut() = None;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(null))));

        let mut opt: Option<i32> = None;
        let mut ob = opt.__observe();
        *ob.tracked_mut() = Some(42);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(42))));

        let mut opt: Option<i32> = None;
        let mut ob = opt.__observe();
        *ob.tracked_mut() = Some(42);
        *ob.tracked_mut() = None;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);

        let mut opt: Option<&str> = Some("42");
        let mut ob = opt.__observe();
        *ob.tracked_mut() = None;
        *ob.tracked_mut() = Some("42");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("42"))));
    }

    #[test]
    fn insert_returns_observer() {
        let mut opt: Option<String> = None;
        let mut ob = opt.__observe();
        let s = ob.insert(String::from("99"));
        assert_eq!(format!("{s:?}"), r#"StringObserver("99")"#);
        *s += "9";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("999"))));
    }

    #[test]
    fn as_mut_tracks_inner() {
        let mut opt = Some(String::from("foo"));
        let mut ob = opt.__observe();
        *ob.as_mut().unwrap() += "bar";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("bar"))));
    }

    #[test]
    fn get_or_insert() {
        // get_or_insert
        let mut opt: Option<i32> = None;
        let mut ob = opt.__observe();
        *ob.get_or_insert(5) = 6;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(6))));

        // get_or_insert_default
        let mut opt: Option<i32> = None;
        let mut ob = opt.__observe();
        *ob.get_or_insert_default() = 77;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(77))));

        // get_or_insert_with
        let mut opt: Option<i32> = None;
        let mut ob = opt.__observe();
        *ob.get_or_insert_with(|| 88) = 99;
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(99))));
    }

    #[test]
    fn specialization() {
        let mut opt: Option<i32> = Some(0i32);
        let ob: GeneralObserver<_, _, _> = opt.__observe();
        assert_eq!(format!("{ob:?}"), r#"SnapshotObserver(Some(0))"#);

        let mut opt: Option<&str> = Some("");
        let ob: OptionObserver<_, _, _> = opt.__observe();
        assert_eq!(format!("{ob:?}"), r#"OptionObserver(Some(""))"#);
    }

    #[test]
    fn ref_specialization() {
        let mut opt = &Some(0i32);
        let ob = opt.__observe();
        assert_eq!(format!("{ob:?}"), r#"DerefObserver(SnapshotObserver(Some(0)))"#);

        let mut opt = &Some("");
        let ob = opt.__observe();
        assert_eq!(format!("{ob:?}"), r#"DerefObserver(OptionObserver(Some("")))"#);
    }

    #[test]
    fn relocate_provenance_mut() {
        let mut vec: Vec<Option<&str>> = vec![Some("hello")];
        let mut ob = vec.__observe();
        *ob[0].tracked_mut() = Some("world");
        ob.reserve(10); // force reallocation, triggers relocate
        *ob[0].tracked_mut() = Some("after");
        assert_eq!(*ob[0].untracked_ref(), Some("after"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(["after"]))));
    }

    #[test]
    fn relocate_provenance_ref() {
        let mut vec = vec![Some(String::from("hello"))];
        let mut ob = vec.__observe();
        // Access element to create inner OptionObserver with inner StringObserver
        assert_eq!(*ob[0].untracked_ref(), Some(String::from("hello")));
        // Flush relocates the OptionObserver with a shared-provenance pointer
        // (VecObserverState::flush uses slice.iter() + cast to *mut).
        // This would fail under Miri if relocate used .as_deref_mut() internally.
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }
}
