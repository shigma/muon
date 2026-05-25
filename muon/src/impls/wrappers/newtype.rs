use std::cmp::Reverse;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::num::{Saturating, Wrapping};
use std::ops::{Deref, DerefMut};

use crate::Mutations;
use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::{spec_impl_observe, spec_impl_ro_observe};
use crate::helper::{AsDeref, AsDerefMut, AsDerefPtrExt, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{Observer, SerializeObserver};

/// Helper trait to access the inner field of a transparent newtype wrapper.
pub trait Newtype {
    type Inner;

    fn as_inner(&self) -> &Self::Inner;
    fn as_inner_mut(&mut self) -> &mut Self::Inner;
    fn as_inner_ptr(this: *mut Self) -> *mut Self::Inner;
}

impl<T> Newtype for Wrapping<T> {
    type Inner = T;

    fn as_inner(&self) -> &T {
        &self.0
    }

    fn as_inner_mut(&mut self) -> &mut T {
        &mut self.0
    }

    fn as_inner_ptr(this: *mut Self) -> *mut T {
        unsafe { &raw mut (*this).0 }
    }
}

impl<T> Newtype for Saturating<T> {
    type Inner = T;

    fn as_inner(&self) -> &T {
        &self.0
    }

    fn as_inner_mut(&mut self) -> &mut T {
        &mut self.0
    }

    fn as_inner_ptr(this: *mut Self) -> *mut T {
        unsafe { &raw mut (*this).0 }
    }
}

impl<T> Newtype for Reverse<T> {
    type Inner = T;

    fn as_inner(&self) -> &T {
        &self.0
    }

    fn as_inner_mut(&mut self) -> &mut T {
        &mut self.0
    }

    fn as_inner_ptr(this: *mut Self) -> *mut T {
        unsafe { &raw mut (*this).0 }
    }
}

/// Observer implementation for transparent newtype wrappers such as
/// [`Wrapping<T>`], [`Saturating<T>`], and [`Reverse<T>`].
pub struct NewtypeObserver<O, S: ?Sized, D = Zero>(pub O, Pointer<S>, PhantomData<D>);

impl<O, S: ?Sized, D> Deref for NewtypeObserver<O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.1
    }
}

impl<O, S: ?Sized, D> DerefMut for NewtypeObserver<O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.1);
        &mut self.1
    }
}

impl<O, S: ?Sized, D> QuasiObserver for NewtypeObserver<O, S, D>
where
    O: QuasiObserver,
    D: Unsigned,
    S: AsDeref<D>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        O::invalidate(&mut this.0);
    }
}

impl<O, S: ?Sized, D> Observer for NewtypeObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target: Newtype<Inner = O::Head>>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
{
    unsafe fn observe(head: *mut Self::Head) -> Self {
        unsafe {
            let value = head.as_deref_ptr::<D>();
            let ob = O::observe(Newtype::as_inner_ptr(value));
            let ptr = Pointer::new_unchecked(head);
            let this = Self(ob, ptr, PhantomData);
            Pointer::register_observer(&this.1, &this.0);
            this
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe {
            let value = head.as_deref_ptr::<D>();
            O::relocate(&mut this.0, Newtype::as_inner_ptr(value));
            Pointer::set_unchecked(&this.1, head);
        }
    }
}

impl<O, S: ?Sized, D> SerializeObserver for NewtypeObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target: Newtype<Inner = O::Head>>,
    O: SerializeObserver<InnerDepth = Zero, Head: Sized>,
{
    fn flush(this: &mut Self) -> Mutations {
        SerializeObserver::flush(&mut this.0)
    }

    fn flat_flush(this: &mut Self) -> Mutations {
        SerializeObserver::flat_flush(&mut this.0)
    }
}

impl<O, S: ?Sized, D> Debug for NewtypeObserver<O, S, D>
where
    O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
    D: Unsigned,
    S: AsDeref<D>,
    S::Target: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NewtypeObserver").field(&self.untracked_ref()).finish()
    }
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<NewtypeObserver<O2, S2, D2>> for NewtypeObserver<O1, S1, D1>
where
    O1: QuasiObserver<Target: Deref<Target: AsDeref<O1::InnerDepth>>>,
    O2: QuasiObserver<Target: Deref<Target: AsDeref<O2::InnerDepth>>>,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1>,
    S2: AsDeref<D2>,
    S1::Target: PartialEq<S2::Target>,
{
    fn eq(&self, other: &NewtypeObserver<O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Eq for NewtypeObserver<O, S, D>
where
    O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
    D: Unsigned,
    S: AsDeref<D>,
    S::Target: Eq,
{
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<NewtypeObserver<O2, S2, D2>> for NewtypeObserver<O1, S1, D1>
where
    O1: QuasiObserver<Target: Deref<Target: AsDeref<O1::InnerDepth>>>,
    O2: QuasiObserver<Target: Deref<Target: AsDeref<O2::InnerDepth>>>,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1>,
    S2: AsDeref<D2>,
    S1::Target: PartialOrd<S2::Target>,
{
    fn partial_cmp(&self, other: &NewtypeObserver<O2, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Ord for NewtypeObserver<O, S, D>
where
    O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
    D: Unsigned,
    S: AsDeref<D>,
    S::Target: Ord,
{
    fn cmp(&self, other: &NewtypeObserver<O, S, D>) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

macro_rules! impl_fmt {
    ($($trait:ident),* $(,)?) => {
        $(
            impl<O, S: ?Sized, D> std::fmt::$trait for NewtypeObserver<O, S, D>
            where
                O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
                D: Unsigned,
                S: AsDeref<D>,
                S::Target: std::fmt::$trait,
            {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    std::fmt::$trait::fmt(self.untracked_ref(), f)
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
    UpperExp,
    UpperHex,
}

macro_rules! impl_ops_assign {
    ($($trait:ident => $method:ident),* $(,)?) => {
        $(
            impl<O, S: ?Sized, D, U> std::ops::$trait<U> for NewtypeObserver<O, S, D>
            where
                S: AsDerefMut<D>,
                O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
                D: Unsigned,
                S::Target: std::ops::$trait<U>,
            {
                fn $method(&mut self, rhs: U) {
                    self.tracked_mut().$method(rhs);
                }
            }
        )*
    };
}

impl_ops_assign! {
    AddAssign => add_assign,
    SubAssign => sub_assign,
    MulAssign => mul_assign,
    DivAssign => div_assign,
    RemAssign => rem_assign,
    BitAndAssign => bitand_assign,
    BitOrAssign => bitor_assign,
    BitXorAssign => bitxor_assign,
    ShlAssign => shl_assign,
    ShrAssign => shr_assign,
}

macro_rules! impl_ops_copy {
    ($($trait:ident => $method:ident),* $(,)?) => {
        $(
            impl<O, S: ?Sized, D, U> std::ops::$trait<U> for NewtypeObserver<O, S, D>
            where
                O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
                S: AsDeref<D>,
                D: Unsigned,
                S::Target: std::ops::$trait<U> + Copy,
            {
                type Output = <S::Target as std::ops::$trait<U>>::Output;

                fn $method(self, rhs: U) -> Self::Output {
                    self.untracked_ref().$method(rhs)
                }
            }
        )*
    };
}

impl_ops_copy! {
    Add => add,
    Sub => sub,
    Mul => mul,
    Div => div,
    Rem => rem,
    BitAnd => bitand,
    BitOr => bitor,
    BitXor => bitxor,
    Shl => shl,
    Shr => shr,
}

macro_rules! impl_ops_copy_unary {
    ($($trait:ident => $method:ident),* $(,)?) => {
        $(
            impl<O, S: ?Sized, D> std::ops::$trait for NewtypeObserver<O, S, D>
            where
                O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
                S: AsDeref<D>,
                D: Unsigned,
                S::Target: std::ops::$trait + Copy,
            {
                type Output = <S::Target as std::ops::$trait>::Output;

                fn $method(self) -> Self::Output {
                    (*self.untracked_ref()).$method()
                }
            }
        )*
    };
}

impl_ops_copy_unary! {
    Neg => neg,
    Not => not,
}

macro_rules! impl_newtype {
    ($helper:ident, $helper_ref:ident, $wrapper:ident) => {
        spec_impl_observe!($helper, $wrapper<Self>, $wrapper<T>, NewtypeObserver);
        spec_impl_ro_observe!($helper_ref, $wrapper<Self>, $wrapper<T>, NewtypeObserver);

        impl<T: Snapshot> Snapshot for $wrapper<T> {
            type Snapshot = T::Snapshot;

            fn to_snapshot(&self) -> Self::Snapshot {
                self.0.to_snapshot()
            }
        }

        impl<T: SerializeSnapshot> SerializeSnapshot for $wrapper<T> {
            fn flush(&self, snapshot: Self::Snapshot) -> Mutations {
                self.0.flush(snapshot)
            }
        }
    };
}

impl_newtype!(WrappingObserveImpl, WrappingRoObserveImpl, Wrapping);
impl_newtype!(SaturatingObserveImpl, SaturatingRoObserveImpl, Saturating);
impl_newtype!(ReverseObserveImpl, ReverseRoObserveImpl, Reverse);

#[cfg(test)]
mod tests {
    use std::cmp::Reverse;
    use std::num::{Saturating, Wrapping};

    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn wrapping_no_change() {
        let mut value = Wrapping(String::from("hello"));
        let mut ob = value.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn wrapping_replace() {
        let mut value = Wrapping(String::from("hello"));
        let mut ob = value.__observe();
        *ob.tracked_mut() = Wrapping(String::from("world"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("world"))));
    }

    #[test]
    fn saturating_no_change() {
        let mut value = Saturating(42u32);
        let mut ob = value.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn saturating_replace() {
        let mut value = Saturating(42u32);
        let mut ob = value.__observe();
        *ob.tracked_mut() = Saturating(100u32);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(100))));
    }

    #[test]
    fn reverse_no_change() {
        let mut value = Reverse(String::from("hello"));
        let mut ob = value.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn reverse_replace() {
        let mut value = Reverse(String::from("hello"));
        let mut ob = value.__observe();
        *ob.tracked_mut() = Reverse(String::from("world"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("world"))));
    }

    #[test]
    fn wrapping_granular_append() {
        let mut value = Wrapping(String::from("hello"));
        let mut ob = value.__observe();
        ob.0.push_str(" world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(" world"))));
    }

    #[test]
    fn reverse_granular_append() {
        let mut value = Reverse(String::from("hello"));
        let mut ob = value.__observe();
        ob.0.push_str(" world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(" world"))));
    }

    #[test]
    fn wrapping_add_assign() {
        let mut value = Wrapping(10u32);
        let mut ob = value.__observe();
        ob += Wrapping(5u32);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(15))));
    }

    #[test]
    fn saturating_sub_assign() {
        let mut value = Saturating(10u32);
        let mut ob = value.__observe();
        ob -= Saturating(3u32);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(7))));
    }
}
