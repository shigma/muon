use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

use crate::general::Snapshot;
use crate::helper::{AsDeref, AsDerefMut, QuasiObserver, Succ, Unsigned};
use crate::observe::{Observer, RefObserve, RefObserver, SerializeObserver};
use crate::{Mutations, Observe};

/// Observer implementation for shared-access pointer types such as [`&T`](reference),
/// [`Rc<T>`](std::rc::Rc), and [`Arc<T>`](std::sync::Arc).
///
/// This observer wraps the inner type's observer and forwards all operations to it, maintaining
/// proper dereference chains for pointer types.
pub struct DerefObserver<O> {
    inner: O,
}

/// Observer implementation for pointer types such as [`Box<T>`] and `&mut T`.
///
/// This observer wraps the inner type's observer and forwards all operations to it, maintaining
/// proper dereference chains for pointer types.
pub struct DerefMutObserver<O> {
    inner: O,
}

impl<O> Deref for DerefObserver<O> {
    type Target = O;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<O> Deref for DerefMutObserver<O> {
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

impl<O> DerefMut for DerefMutObserver<O> {
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

impl<O, D> QuasiObserver for DerefMutObserver<O>
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
    O: RefObserver<InnerDepth = Succ<D>>,
    O::Head: AsDeref<D>,
    D: Unsigned,
{
    fn observe(head: &mut Self::Head) -> Self {
        Self {
            inner: O::observe(head),
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe { O::relocate(&mut this.inner, head) }
    }
}

impl<O, D> RefObserver for DerefObserver<O>
where
    O: RefObserver<InnerDepth = Succ<D>>,
    O::Head: AsDeref<D>,
    D: Unsigned,
{
    fn observe(head: &Self::Head) -> Self {
        Self {
            inner: O::observe(head),
        }
    }

    unsafe fn relocate(this: &mut Self, head: *const Self::Head) {
        unsafe { O::relocate(&mut this.inner, head) }
    }
}

impl<O, D> Observer for DerefMutObserver<O>
where
    O: Observer<InnerDepth = Succ<D>>,
    O::Head: AsDeref<D>,
    D: Unsigned,
{
    fn observe(head: &mut Self::Head) -> Self {
        Self {
            inner: O::observe(head),
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
    unsafe fn flush(this: &mut Self) -> Mutations {
        unsafe { O::flush(&mut this.inner) }
    }

    unsafe fn flat_flush(this: &mut Self) -> Mutations {
        unsafe { O::flat_flush(&mut this.inner) }
    }
}

impl<O, D> SerializeObserver for DerefMutObserver<O>
where
    O: SerializeObserver<InnerDepth = Succ<D>>,
    O::Head: AsDeref<D>,
    D: Unsigned,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        unsafe { O::flush(&mut this.inner) }
    }

    unsafe fn flat_flush(this: &mut Self) -> Mutations {
        unsafe { O::flat_flush(&mut this.inner) }
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

            impl<O> std::fmt::$trait for DerefMutObserver<O>
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

impl<O> Debug for DerefMutObserver<O>
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

impl<O1, O2> PartialEq<DerefMutObserver<O2>> for DerefMutObserver<O1>
where
    O1: PartialEq<O2>,
{
    fn eq(&self, other: &DerefMutObserver<O2>) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<O> Eq for DerefObserver<O> where O: Eq {}

impl<O> Eq for DerefMutObserver<O> where O: Eq {}

impl<O1, O2> PartialOrd<DerefObserver<O2>> for DerefObserver<O1>
where
    O1: PartialOrd<O2>,
{
    fn partial_cmp(&self, other: &DerefObserver<O2>) -> Option<std::cmp::Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<O1, O2> PartialOrd<DerefMutObserver<O2>> for DerefMutObserver<O1>
where
    O1: PartialOrd<O2>,
{
    fn partial_cmp(&self, other: &DerefMutObserver<O2>) -> Option<std::cmp::Ordering> {
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

impl<O> Ord for DerefMutObserver<O>
where
    O: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

macro_rules! impl_deref_observe {
    ($(impl $([$($gen:tt)*])? Observe for $ty:ty as $ob:ident $(where { $($where:tt)+ })?;)*) => {
        $(
            impl <$($($gen)*)?> Observe for $ty {
                type Observer<'ob, S, D>
                    = $ob<T::Observer<'ob, S, Succ<D>>>
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
    impl [T: Observe + ?Sized] Observe for Box<T> as DerefMutObserver;
    impl [T: Observe + ?Sized] Observe for &mut T as DerefMutObserver;
    impl [T: RefObserve + ?Sized] Observe for &T as DerefObserver;
    impl [T: RefObserve + ?Sized] Observe for std::rc::Rc<T> as DerefObserver;
    impl [T: RefObserve + ?Sized] Observe for std::sync::Arc<T> as DerefObserver;
}

macro_rules! impl_deref_ref_observe {
    ($(impl $([$($gen:tt)*])? RefObserve for $ty:ty $(where { $($where:tt)+ })?;)*) => {
        $(
            impl <$($($gen)*)?> RefObserve for $ty {
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

impl_deref_ref_observe! {
    impl [T: RefObserve + ?Sized] RefObserve for &T;
    impl [T: RefObserve + ?Sized] RefObserve for &mut T;
    impl [T: RefObserve + ?Sized] RefObserve for Box<T>;
    impl [T: RefObserve + ?Sized] RefObserve for std::rc::Rc<T>;
    impl [T: RefObserve + ?Sized] RefObserve for std::sync::Arc<T>;
}

macro_rules! impl_snapshot {
    ($(impl $([$($gen:tt)*])? Snapshot for $ty:ty as $value:ty $(where { $($where:tt)+ })?;)*) => {
        $(
            impl <$($($gen)*)?> Snapshot for $ty {
                type Snapshot = $value;

                fn to_snapshot(&self) -> Self::Snapshot {
                    (**self).to_snapshot()
                }

                fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
                    (**self).eq_snapshot(snapshot)
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

macro_rules! generic_impl_cmp {
    ($(impl $([$($gen:tt)*])? _ for $ty:ty);* $(;)?) => {
        $(
            impl<$($($gen)*,)? O, D, T: ?Sized> PartialEq<$ty> for DerefObserver<O>
            where
                O: QuasiObserver<InnerDepth = Succ<D>>,
                O::Head: AsDeref<D, Target = T>,
                T: PartialEq<$ty>,
                D: Unsigned,
            {
                fn eq(&self, other: &$ty) -> bool {
                    self.untracked_ref().eq(other)
                }
            }

            impl<$($($gen)*,)? O, D, T: ?Sized> PartialEq<$ty> for DerefMutObserver<O>
            where
                O: QuasiObserver<InnerDepth = Succ<D>>,
                O::Head: AsDeref<D, Target = T>,
                T: PartialEq<$ty>,
                D: Unsigned,
            {
                fn eq(&self, other: &$ty) -> bool {
                    self.untracked_ref().eq(other)
                }
            }

            impl<$($($gen)*,)? O, D, T: ?Sized> PartialOrd<$ty> for DerefObserver<O>
            where
                O: QuasiObserver<InnerDepth = Succ<D>>,
                O::Head: AsDeref<D, Target = T>,
                T: PartialOrd<$ty>,
                D: Unsigned,
            {
                fn partial_cmp(&self, other: &$ty) -> Option<std::cmp::Ordering> {
                    self.untracked_ref().partial_cmp(other)
                }
            }

            impl<$($($gen)*,)? O, D, T: ?Sized> PartialOrd<$ty> for DerefMutObserver<O>
            where
                O: QuasiObserver<InnerDepth = Succ<D>>,
                O::Head: AsDeref<D, Target = T>,
                T: PartialOrd<$ty>,
                D: Unsigned,
            {
                fn partial_cmp(&self, other: &$ty) -> Option<std::cmp::Ordering> {
                    self.untracked_ref().partial_cmp(other)
                }
            }
        )*
    };
}

generic_impl_cmp! {
    impl [U: ?Sized] _ for Box<U>;
    impl ['a, U: ?Sized] _ for &'a U;
    impl ['a, U: ?Sized] _ for &'a mut U;
    impl [U: ?Sized] _ for std::rc::Rc<U>;
    impl [U: ?Sized] _ for std::sync::Arc<U>;
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
        assert_eq!(*ob, "Hello, World!");

        ob.push_str("\n");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("\n"))));
    }

    #[test]
    fn test_deref_replace() {
        let mut value = Box::new(String::from("Hello, World!"));
        let mut ob = value.__observe();
        assert_eq!(*ob, "Hello, World!");

        **ob.tracked_mut() = String::from("42");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("42"))));
    }

    #[test]
    fn test_deref_assign() {
        let mut value = Box::new(String::from("Hello, World!"));
        let mut ob = value.__observe();
        assert_eq!(*ob, "Hello, World!");

        **ob.tracked_mut() = String::from("42");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("42"))));
    }
}
