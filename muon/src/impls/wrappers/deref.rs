use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::{AsDeref, AsDerefMut, QuasiObserver, Succ, Unsigned};
use crate::observe::{Observer, RoObserve, SerializeObserver};
use crate::{Mutations, Observe};

/// Observer implementation for pointer types such as [`&T`](reference),
/// [`Rc<T>`](std::rc::Rc), [`Arc<T>`](std::sync::Arc), [`Box<T>`], and `&mut T`.
///
/// This observer wraps the inner type's observer and forwards all operations to it, maintaining
/// proper dereference chains for pointer types.
pub struct DerefObserver<O> {
    inner: O,
}

impl<O> Deref for DerefObserver<O> {
    type Target = O;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<O> DerefMut for DerefObserver<O> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<O, D> QuasiObserver for DerefObserver<O>
where
    D: Unsigned,
    O: QuasiObserver<InnerDepth = Succ<D>>,
    O::Head: AsDeref<D>,
{
    type Head = O::Head;
    type OuterDepth = Succ<O::OuterDepth>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        O::invalidate(&mut this.inner);
    }
}

impl<O, D> Observer for DerefObserver<O>
where
    O: Observer<InnerDepth = Succ<D>>,
    O::Head: AsDeref<D>,
    D: Unsigned,
{
    unsafe fn observe(head: *mut Self::Head) -> Self {
        Self {
            inner: unsafe { O::observe(head) },
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe { O::relocate(&mut this.inner, head) }
    }
}

impl<O, D> SerializeObserver for DerefObserver<O>
where
    O: SerializeObserver<InnerDepth = Succ<D>>,
    O::Head: AsDeref<D>,
    D: Unsigned,
{
    fn flush(this: &mut Self) -> Mutations {
        O::flush(&mut this.inner)
    }

    fn flat_flush(this: &mut Self) -> Mutations {
        O::flat_flush(&mut this.inner)
    }
}

macro_rules! impl_fmt {
    ($($trait:ident),* $(,)?) => {
        $(
            impl<O> std::fmt::$trait for DerefObserver<O>
            where
                O: std::fmt::$trait,
            {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    std::fmt::$trait::fmt(&self.inner, f)
                }
            }
        )*
    };
}

impl_fmt! {
    Binary,
    Display,
    LowerExp,
    LowerHex,
    Octal,
    Pointer,
    UpperExp,
    UpperHex,
}

impl<O> Debug for DerefObserver<O>
where
    O: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DerefObserver").field(&self.inner).finish()
    }
}

impl<O1, O2> PartialEq<DerefObserver<O2>> for DerefObserver<O1>
where
    O1: PartialEq<O2>,
{
    fn eq(&self, other: &DerefObserver<O2>) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<O> Eq for DerefObserver<O> where O: Eq {}

impl<O1, O2> PartialOrd<DerefObserver<O2>> for DerefObserver<O1>
where
    O1: PartialOrd<O2>,
{
    fn partial_cmp(&self, other: &DerefObserver<O2>) -> Option<std::cmp::Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<O> Ord for DerefObserver<O>
where
    O: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

macro_rules! impl_deref_observe {
    ($(impl $([$($gen:tt)*])? Observe for $ty:ty $(where { $($where:tt)+ })?;)*) => {
        $(
            impl <$($($gen)*)?> Observe for $ty {
                type Observer<'ob, S, D>
                    = DerefObserver<T::Observer<'ob, S, Succ<D>>>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

                type Spec = T::Spec;
            }
        )*
    };
}

impl_deref_observe! {
    impl [T: Observe + ?Sized] Observe for Box<T>;
    impl [T: Observe + ?Sized] Observe for &mut T;
    impl [T: RoObserve + ?Sized] Observe for &T;
    impl [T: RoObserve + ?Sized] Observe for std::rc::Rc<T>;
    impl [T: RoObserve + ?Sized] Observe for std::sync::Arc<T>;
}

macro_rules! impl_deref_ro_observe {
    ($(impl $([$($gen:tt)*])? RoObserve for $ty:ty $(where { $($where:tt)+ })?;)*) => {
        $(
            impl <$($($gen)*)?> RoObserve for $ty {
                type Observer<'ob, S, D>
                    = DerefObserver<T::Observer<'ob, S, Succ<D>>>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDeref<D, Target = Self> + ?Sized + 'ob;

                type Spec = T::Spec;
            }
        )*
    };
}

impl_deref_ro_observe! {
    impl [T: RoObserve + ?Sized] RoObserve for &T;
    impl [T: RoObserve + ?Sized] RoObserve for &mut T;
    impl [T: RoObserve + ?Sized] RoObserve for Box<T>;
    impl [T: RoObserve + ?Sized] RoObserve for std::rc::Rc<T>;
    impl [T: RoObserve + ?Sized] RoObserve for std::sync::Arc<T>;
}

macro_rules! impl_snapshot {
    ($(impl $([$($gen:tt)*])? Snapshot for $ty:ty as $value:ty $(where { $($where:tt)+ })?;)*) => {
        $(
            impl <$($($gen)*)?> Snapshot for $ty {
                type Snapshot = $value;

                fn to_snapshot(&self) -> Self::Snapshot {
                    (**self).to_snapshot()
                }
            }
        )*
    };
}

impl_snapshot! {
    impl [T: Snapshot + ?Sized] Snapshot for &T as T::Snapshot;
    impl [T: Snapshot + ?Sized] Snapshot for &mut T as T::Snapshot;
    impl [T: Snapshot + ?Sized] Snapshot for Box<T> as T::Snapshot;
    impl [T: Snapshot + ?Sized] Snapshot for std::rc::Rc<T> as T::Snapshot;
    impl [T: Snapshot + ?Sized] Snapshot for std::sync::Arc<T> as T::Snapshot;
}

macro_rules! impl_serialize_snapshot {
    ($(impl $([$($gen:tt)*])? SerializeSnapshot for $ty:ty;)*) => {
        $(
            impl <$($($gen)*)?> SerializeSnapshot for $ty {
                fn flush(&self, snapshot: Self::Snapshot) -> Mutations {
                    (**self).flush(snapshot)
                }
            }
        )*
    };
}

impl_serialize_snapshot! {
    impl [T: SerializeSnapshot + ?Sized] SerializeSnapshot for &T;
    impl [T: SerializeSnapshot + ?Sized] SerializeSnapshot for &mut T;
    impl [T: SerializeSnapshot + ?Sized] SerializeSnapshot for Box<T>;
    impl [T: SerializeSnapshot + ?Sized] SerializeSnapshot for std::rc::Rc<T>;
    impl [T: SerializeSnapshot + ?Sized] SerializeSnapshot for std::sync::Arc<T>;
}

#[cfg(test)]
mod test {
    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn test_deref_method() {
        let mut value = Box::new(String::from("Hello, World!"));
        let mut ob = value.__observe();
        assert_eq!(**ob.untracked_ref(), "Hello, World!");

        ob.push_str("\n");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("\n"))));
    }

    #[test]
    fn test_deref_replace() {
        let mut value = Box::new(String::from("Hello, World!"));
        let mut ob = value.__observe();
        assert_eq!(**ob.untracked_ref(), "Hello, World!");

        **ob.tracked_mut() = String::from("42");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("42"))));
    }

    #[test]
    fn test_deref_assign() {
        let mut value = Box::new(String::from("Hello, World!"));
        let mut ob = value.__observe();
        assert_eq!(**ob.untracked_ref(), "Hello, World!");

        **ob.tracked_mut() = String::from("42");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("42"))));
    }
}
