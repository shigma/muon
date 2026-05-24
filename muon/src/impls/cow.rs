use std::borrow::Cow;
use std::fmt::Debug;
use std::ops::{AddAssign, Deref, DerefMut};

use crate::general::Snapshot;
use crate::helper::{AsDeref, AsDerefMut, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::impls::{DerefObserver, StringObserver};
use crate::observe::{DefaultSpec, Observer, RefObserve, RefObserver, SerializeObserver};
use crate::{Mutations, Observe};

/// Observer implementation for [`Cow<'a, T>`].
pub struct CowObserver<B, O> {
    inner: B,
    owned: Option<O>,
    mutated: bool,
}

impl<B, O> Deref for CowObserver<B, O> {
    type Target = B;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<B, O> DerefMut for CowObserver<B, O> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.owned = None;
        self.mutated = true;
        &mut self.inner
    }
}

impl<B, O, D> QuasiObserver for CowObserver<B, O>
where
    D: Unsigned,
    B: QuasiObserver<InnerDepth = Succ<D>>,
    B::Head: AsDeref<D>,
{
    type Head = B::Head;
    type OuterDepth = Succ<B::OuterDepth>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        this.owned = None;
        this.mutated = true;
        B::invalidate(&mut this.inner);
    }
}

impl<'a, B, O, D, T> Observer for CowObserver<B, O>
where
    B: RefObserver<InnerDepth = Succ<D>>,
    B::Head: AsDerefMut<D, Target = Cow<'a, T>>,
    O: Observer<InnerDepth = Zero, Head = T::Owned>,
    D: Unsigned,
    T: ToOwned + ?Sized + 'a,
{
    fn observe(head: &mut Self::Head) -> Self {
        let inner = B::observe(head);
        let owned = match AsDerefMut::<D>::as_deref_mut(head) {
            Cow::Borrowed(_) => None,
            Cow::Owned(value) => Some(O::observe(value)),
        };
        // B is a RefObserver so its Pointer only has shared provenance.
        // Re-expose with mutable provenance for later Pointer::as_mut calls.
        Pointer::set(inner.as_deref_coinductive(), &mut *head);
        Self {
            inner,
            owned,
            mutated: false,
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe { B::relocate(&mut this.inner, head) }
        if let Some(owned) = &mut this.owned {
            match unsafe { AsDerefMut::<D>::as_deref_mut(&mut *head) } {
                Cow::Borrowed(_) => panic!("inconsistent state for CowObserver"),
                Cow::Owned(value) => unsafe { O::relocate(owned, value) },
            }
        }
        // Re-expose with mutable provenance (see observe for rationale).
        Pointer::set(this.inner.as_deref_coinductive(), unsafe { &mut *head });
    }
}

impl<'a, B, O, D, T> SerializeObserver for CowObserver<B, O>
where
    D: Unsigned,
    B: SerializeObserver<InnerDepth = Succ<D>>,
    B::Head: AsDeref<D, Target = Cow<'a, T>>,
    O: SerializeObserver<InnerDepth = Zero, Head = T::Owned>,
    T: ToOwned + ?Sized + 'a,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        if let Some(owned) = this.owned.as_mut()
            && !this.mutated
        {
            unsafe { B::flush(&mut this.inner) };
            unsafe { O::flush(owned) }
        } else {
            this.owned = None;
            this.mutated = false;
            unsafe { B::flush(&mut this.inner) }
        }
    }

    unsafe fn flat_flush(this: &mut Self) -> Mutations {
        if let Some(owned) = this.owned.as_mut()
            && !this.mutated
        {
            unsafe { B::flat_flush(&mut this.inner) };
            unsafe { O::flat_flush(owned) }
        } else {
            this.owned = None;
            this.mutated = false;
            unsafe { B::flat_flush(&mut this.inner) }
        }
    }
}

impl<'a, B, O, T, D> CowObserver<B, O>
where
    D: Unsigned,
    B: Observer<InnerDepth = Succ<D>>,
    B::Head: AsDerefMut<D, Target = Cow<'a, T>>,
    O: Observer<InnerDepth = Zero, Head = T::Owned>,
    T: ToOwned + ?Sized + 'a,
{
    /// See [`Cow::to_mut`].
    pub fn to_mut(&mut self) -> &mut O {
        let head = unsafe { Pointer::as_mut(self.inner.as_deref_coinductive()) };
        let cow = AsDerefMut::<D>::as_deref_mut(head);
        let owned = cow.to_mut();
        self.owned.get_or_insert_with(|| O::observe(owned))
    }
}

impl<B, O> Debug for CowObserver<B, O>
where
    B: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CowObserver").field(&self.inner).finish()
    }
}

impl<B1, B2, O1, O2> PartialEq<CowObserver<B2, O2>> for CowObserver<B1, O1>
where
    B1: PartialEq<B2>,
{
    fn eq(&self, other: &CowObserver<B2, O2>) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<B, O> Eq for CowObserver<B, O> where B: Eq {}

impl<B1, B2, O1, O2> PartialOrd<CowObserver<B2, O2>> for CowObserver<B1, O1>
where
    B1: PartialOrd<B2>,
{
    fn partial_cmp(&self, other: &CowObserver<B2, O2>) -> Option<std::cmp::Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<B, O> Ord for CowObserver<B, O>
where
    B: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl<'a, 'ob, B, D, R> AddAssign<R> for CowObserver<B, StringObserver<'ob, String, Zero>>
where
    D: Unsigned,
    B: Observer<InnerDepth = Succ<D>>,
    B::Head: AsDerefMut<D, Target = Cow<'a, str>>,
    Cow<'a, str>: AddAssign<R>,
    R: Deref<Target = str> + Into<Cow<'a, str>>,
{
    fn add_assign(&mut self, rhs: R) {
        let head = unsafe { Pointer::as_mut(self.inner.as_deref_coinductive()) };
        let cow = AsDerefMut::<D>::as_deref_mut(head);
        if cow.is_empty() {
            self.mutated = true;
            B::invalidate(&mut self.inner);
            *cow = rhs.into();
        } else if !rhs.is_empty() {
            if let Cow::Borrowed(lhs) = cow {
                let mut s = String::with_capacity(lhs.len() + rhs.len());
                s.push_str(lhs);
                *cow = Cow::Owned(s);
            }
            self.to_mut().push_str(&rhs);
        }
    }
}

macro_rules! generic_impl_cmp {
    ($(impl $([$($gen:tt)*])? _ for $ty:ty);* $(;)?) => {
        $(
            impl<'a, $($($gen)*,)? B, O, T, D> PartialEq<$ty> for CowObserver<B, O>
            where
                D: Unsigned,
                B: Observer<InnerDepth = Succ<D>>,
                B::Head: AsDerefMut<D, Target = Cow<'a, T>>,
                T: ToOwned + ?Sized + 'a,
                Cow<'a, T>: PartialEq<$ty>,
            {
                fn eq(&self, other: &$ty) -> bool {
                    self.untracked_ref().eq(other)
                }
            }

            impl<'a, $($($gen)*,)? B, O, T, D> PartialOrd<$ty> for CowObserver<B, O>
            where
                D: Unsigned,
                B: Observer<InnerDepth = Succ<D>>,
                B::Head: AsDerefMut<D, Target = Cow<'a, T>>,
                T: ToOwned + ?Sized + 'a,
                Cow<'a, T>: PartialOrd<$ty>,
            {
                fn partial_cmp(&self, other: &$ty) -> Option<std::cmp::Ordering> {
                    self.untracked_ref().partial_cmp(other)
                }
            }
        )*
    };
}

generic_impl_cmp! {
    impl _ for str;
    impl _ for String;
    impl _ for std::ffi::CStr;
    impl _ for std::ffi::CString;
    impl _ for std::ffi::OsStr;
    impl _ for std::ffi::OsString;
    impl _ for std::path::Path;
    impl _ for std::path::PathBuf;
    impl [U] _ for Vec<U>;
    impl ['b, U: ?Sized] _ for &'b U;
    impl ['b, U: ?Sized] _ for &'b mut U;
    impl ['b, U: ToOwned + ?Sized] _ for Cow<'b, U>;
}

impl<'a, T> Observe for Cow<'a, T>
where
    T: ToOwned + RefObserve + ?Sized + 'a,
    T::Owned: Observe,
{
    type Observer<'ob, S, D>
        = CowObserver<T::Observer<'ob, S, Succ<D>>, <T::Owned as Observe>::Observer<'ob, T::Owned, Zero>>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl<'a, T> RefObserve for Cow<'a, T>
where
    T: RefObserve + ToOwned + ?Sized + 'a,
{
    type Observer<'ob, S, D>
        = DerefObserver<T::Observer<'ob, S, Succ<D>>>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = T::Spec;
}

impl<'a, T> Snapshot for Cow<'a, T>
where
    T: Snapshot + ToOwned + ?Sized,
{
    type Snapshot = T::Snapshot;

    fn to_snapshot(&self) -> Self::Snapshot {
        (**self).to_snapshot()
    }

    fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
        (**self).eq_snapshot(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use muon_test_utils::*;
    use serde_json::json;

    use super::*;
    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_change_returns_none() {
        let mut cow = Cow::Borrowed("hello");
        let mut ob = cow.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_via_deref_mut() {
        let mut cow = Cow::Borrowed("hello");
        let mut ob = cow.__observe();
        *ob.tracked_mut() = Cow::Owned(String::from("world"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("world"))));
    }

    #[test]
    fn unsize_append() {
        const S: &str = "hello world";
        let mut cow = Cow::Borrowed(&S[..5]);
        let mut ob = cow.__observe();
        *ob.tracked_mut() = Cow::Borrowed(S);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(" world"))));
    }

    #[test]
    fn unsize_truncate() {
        const S: &str = "hello world";
        let mut cow = Cow::Borrowed(S);
        let mut ob = cow.__observe();
        *ob.tracked_mut() = Cow::Borrowed(&S[..5]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 6)));
    }

    #[test]
    fn to_mut_no_change() {
        let mut cow = Cow::Borrowed("hello");
        let mut ob = cow.__observe();
        ob.to_mut();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn to_mut_granular_tracking() {
        let mut cow = Cow::Borrowed("hello");
        let mut ob = cow.__observe();
        ob.to_mut().push_str(" world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(" world"))));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_after_to_mut() {
        let mut cow = Cow::Borrowed("hello");
        let mut ob = cow.__observe();
        ob.to_mut().push_str(" world");
        *ob.tracked_mut() = Cow::Borrowed("replaced");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("replaced"))));
    }

    #[test]
    fn to_mut_after_replace() {
        let mut cow = Cow::Borrowed("hello");
        let mut ob = cow.__observe();
        *ob.tracked_mut() = Cow::Borrowed("replaced");
        ob.to_mut().push_str(" world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("replaced world"))));
    }

    #[test]
    fn owned_cow_no_change() {
        let mut cow: Cow<'_, str> = Cow::Owned(String::from("hello"));
        let mut ob = cow.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn owned_cow_replace() {
        let mut cow: Cow<'_, str> = Cow::Owned(String::from("hello"));
        let mut ob = cow.__observe();
        *ob.tracked_mut() = Cow::Owned(String::from("world"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("world"))));
    }

    #[test]
    fn comparison_with_cow() {
        let mut cow = Cow::Borrowed("hello");
        let ob = cow.__observe();
        assert_eq!(ob, Cow::Borrowed("hello"));
        assert_eq!(ob, Cow::<str>::Owned(String::from("hello")));
    }

    #[test]
    fn to_mut_truncate_then_append() {
        let mut cow: Cow<'_, str> = Cow::Owned(String::from("hello world"));
        let mut ob = cow.__observe();
        let s = ob.to_mut();
        s.truncate(5);
        s.push_str("!");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 6), append!(_, json!("!")))));
    }

    #[test]
    fn add_assign_borrowed() {
        let mut cow = Cow::Borrowed("hello");
        let mut ob = cow.__observe();
        ob += " world";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(" world"))));
    }

    #[test]
    fn add_assign_empty_lhs() {
        let mut cow = Cow::Borrowed("");
        let mut ob = cow.__observe();
        ob += "hello";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("hello"))));
    }

    #[test]
    fn add_assign_empty_rhs() {
        let mut cow = Cow::Borrowed("hello");
        let mut ob = cow.__observe();
        ob += "";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn add_assign_owned() {
        let mut cow: Cow<'_, str> = Cow::Owned(String::from("hello"));
        let mut ob = cow.__observe();
        ob += " world";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(" world"))));
    }

    #[test]
    fn add_assign_multiple() {
        let mut cow = Cow::Borrowed("a");
        let mut ob = cow.__observe();
        ob += "b";
        ob += "c";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("bc"))));
    }
}
