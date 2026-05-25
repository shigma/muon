use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use serde::Serialize;

use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::{spec_impl_observe, spec_impl_ro_observe};
use crate::helper::{AsDeref, AsDerefMut, AsDerefPtrExt, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{DefaultSpec, Observer, RoObserve, SerializeObserver};
use crate::{Mutations, Observe};

/// Observer implementation for tuple `(T,)`.
pub struct TupleObserver<O, S: ?Sized, D = Zero>(pub O, Pointer<S>, PhantomData<D>);

impl<O, S: ?Sized, D> Deref for TupleObserver<O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.1
    }
}

impl<O, S: ?Sized, D> DerefMut for TupleObserver<O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.1);
        &mut self.1
    }
}

impl<O, S: ?Sized, D> QuasiObserver for TupleObserver<O, S, D>
where
    O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
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

impl<O, S: ?Sized, D> Observer for TupleObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = (O::Head,)>,
    O: Observer<InnerDepth = Zero>,
    O::Head: Sized,
{
    unsafe fn observe(head: *mut Self::Head) -> Self {
        unsafe {
            let tuple = head.as_deref_ptr::<D>();
            let ob = O::observe(&raw mut (*tuple).0);
            let ptr = Pointer::new_unchecked(head);
            let this = Self(ob, ptr, PhantomData);
            Pointer::register_observer(&this.1, &this.0);
            this
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe {
            let tuple = head.as_deref_ptr::<D>();
            O::relocate(&mut this.0, &raw mut (*tuple).0);
            Pointer::set_unchecked(&this.1, head);
        }
    }
}

impl<O, S: ?Sized, D> SerializeObserver for TupleObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = (O::Head,)>,
    O: SerializeObserver<InnerDepth = Zero>,
    O::Head: Serialize + Sized + 'static,
{
    fn flush(this: &mut Self) -> Mutations {
        let mutations_0 = SerializeObserver::flush(&mut this.0);
        if mutations_0.is_replace() {
            Mutations::replace((*this).untracked_ref())
        } else {
            mutations_0.with_prefix(0)
        }
    }
}

impl<O, S: ?Sized, D> Debug for TupleObserver<O, S, D>
where
    O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
    D: Unsigned,
    S: AsDeref<D>,
    S::Target: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TupleObserver").field(&self.untracked_ref()).finish()
    }
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<TupleObserver<O2, S2, D2>> for TupleObserver<O1, S1, D1>
where
    O1: QuasiObserver<Target: Deref<Target: AsDeref<O1::InnerDepth>>>,
    O2: QuasiObserver<Target: Deref<Target: AsDeref<O2::InnerDepth>>>,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1>,
    S2: AsDeref<D2>,
    S1::Target: PartialEq<S2::Target>,
{
    fn eq(&self, other: &TupleObserver<O2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Eq for TupleObserver<O, S, D>
where
    O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
    D: Unsigned,
    S: AsDeref<D>,
    S::Target: Eq,
{
}

impl<O1, O2, S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<TupleObserver<O2, S2, D2>> for TupleObserver<O1, S1, D1>
where
    O1: QuasiObserver<Target: Deref<Target: AsDeref<O1::InnerDepth>>>,
    O2: QuasiObserver<Target: Deref<Target: AsDeref<O2::InnerDepth>>>,
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1>,
    S2: AsDeref<D2>,
    S1::Target: PartialOrd<S2::Target>,
{
    fn partial_cmp(&self, other: &TupleObserver<O2, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<O, S: ?Sized, D> Ord for TupleObserver<O, S, D>
where
    O: QuasiObserver<Target: Deref<Target: AsDeref<O::InnerDepth>>>,
    D: Unsigned,
    S: AsDeref<D>,
    S::Target: Ord,
{
    fn cmp(&self, other: &TupleObserver<O, S, D>) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

spec_impl_observe! {
    #[cfg_attr(docsrs, doc(fake_variadic))]
    #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
    TupleObserveImpl, (Self,), (T,), TupleObserver
}

spec_impl_ro_observe! {
    #[cfg_attr(docsrs, doc(fake_variadic))]
    #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
    TupleRoObserveImpl, (Self,), (T,), TupleObserver
}

#[cfg_attr(docsrs, doc(fake_variadic))]
#[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
impl<T: Snapshot> Snapshot for (T,) {
    type Snapshot = (T::Snapshot,);

    fn to_snapshot(&self) -> Self::Snapshot {
        (self.0.to_snapshot(),)
    }
}

#[cfg_attr(docsrs, doc(fake_variadic))]
#[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
impl<T: SerializeSnapshot + 'static> SerializeSnapshot for (T,) {
    fn flush(&self, snapshot: Self::Snapshot) -> Mutations {
        let mutations = self.0.flush(snapshot.0);
        if mutations.is_replace() {
            Mutations::replace(self)
        } else {
            mutations.with_prefix(0)
        }
    }
}

macro_rules! tuple_observer {
    ($ty:ident; $ptr:tt; $($o:ident, $p:ident, $t:ident, $u:ident, $n:tt);*) => {
        #[doc = concat!("Observer implementation for tuple `", stringify!(($($t),*)), "`.")]
        pub struct $ty<$($o,)* S: ?Sized, D = Zero>(
            $(pub $o,)*
            /* ptr */ Pointer<S>,
            /* phantom */ PhantomData<D>,
        );

        impl<$($o,)* S: ?Sized, D> Deref for $ty<$($o,)* S, D> {
            type Target = Pointer<S>;

            fn deref(&self) -> &Self::Target {
                &self.$ptr
            }
        }

        impl<$($o,)* S: ?Sized, D> DerefMut for $ty<$($o,)* S, D>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
        {
            fn deref_mut(&mut self) -> &mut Self::Target {
                std::ptr::from_mut(self).expose_provenance();
                Pointer::invalidate(&mut self.$ptr);
                &mut self.$ptr
            }
        }

        impl<$($o,)* S: ?Sized, D> QuasiObserver for $ty<$($o,)* S, D>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
            D: Unsigned,
            S: AsDeref<D>,
        {
            type Head = S;
            type OuterDepth = Succ<Zero>;
            type InnerDepth = D;

            fn invalidate(this: &mut Self) {
                $($o::invalidate(&mut this.$n);)*
            }
        }

        impl<$($o,)* S: ?Sized, D> Observer for $ty<$($o,)* S, D>
        where
            D: Unsigned,
            S: AsDeref<D, Target = ($($o::Head,)*)>,
            $($o: Observer<InnerDepth = Zero, Head: Sized>,)*
        {
            unsafe fn observe(head: *mut Self::Head) -> Self {
                unsafe {
                    let tuple = head.as_deref_ptr::<D>();
                    let this = Self(
                        $($o::observe(&raw mut (*tuple).$n),)*
                        /* ptr */ Pointer::new_unchecked(head),
                        /* phantom */ PhantomData,
                    );
                    $(Pointer::register_observer(&this.$ptr, &this.$n);)*
                    this
                }
            }

            unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
                unsafe {
                    let tuple = head.as_deref_ptr::<D>();
                    $($o::relocate(&mut this.$n, &raw mut (*tuple).$n);)*
                    Pointer::set_unchecked(&this.$ptr, head);
                }
            }
        }

        impl<$($o,)* S: ?Sized, D> SerializeObserver for $ty<$($o,)* S, D>
        where
            D: Unsigned,
            S: AsDeref<D, Target = ($($o::Head,)*)>,
            $($o: SerializeObserver<InnerDepth = Zero, Head: Serialize + Sized + 'static>,)*
        {
            fn flush(this: &mut Self) -> Mutations {
                let mutations_tuple = ($(SerializeObserver::flush(&mut this.$n).with_prefix($n),)*);
                let capacity = 0 $(+ mutations_tuple.$n.len())*;
                if capacity == $ptr {
                    return Mutations::replace((*this).untracked_ref());
                }
                let mut mutations = Mutations::new().with_capacity(capacity);
                $(
                    mutations.extend(mutations_tuple.$n);
                )*
                mutations
            }
        }

        impl<$($o,)* S: ?Sized, D> Debug for $ty<$($o,)* S, D>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
            D: Unsigned,
            S: AsDeref<D>,
            S::Target: Debug,
        {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_tuple(stringify!($ty)).field(&self.untracked_ref()).finish()
            }
        }

        impl<$($o,)* S: ?Sized, D, U> PartialEq<(U,)> for $ty<$($o,)* S, D>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
            D: Unsigned,
            S: AsDeref<D>,
            S::Target: PartialEq<(U,)>,
        {
            fn eq(&self, other: &(U,)) -> bool {
                self.untracked_ref().eq(other)
            }
        }

        impl<$($o,)* $($p,)* S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<$ty<$($p,)* S2, D2>>
            for $ty<$($o,)* S1, D1>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
            $($p: QuasiObserver<Target: Deref<Target: AsDeref<$p::InnerDepth>>>,)*
            D1: Unsigned,
            D2: Unsigned,
            S1: AsDeref<D1>,
            S2: AsDeref<D2>,
            S1::Target: PartialEq<S2::Target>,
        {
            fn eq(&self, other: &$ty<$($p,)* S2, D2>) -> bool {
                self.untracked_ref().eq(other.untracked_ref())
            }
        }

        impl<$($o,)* S: ?Sized, D> Eq for $ty<$($o,)* S, D>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
            D: Unsigned,
            S: AsDeref<D>,
            S::Target: Eq,
        {
        }

        impl<$($o,)* S: ?Sized, D, U> PartialOrd<(U,)> for $ty<$($o,)* S, D>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
            D: Unsigned,
            S: AsDeref<D>,
            S::Target: PartialOrd<(U,)>,
        {
            fn partial_cmp(&self, other: &(U,)) -> Option<std::cmp::Ordering> {
                self.untracked_ref().partial_cmp(other)
            }
        }

        impl<$($o,)* $($p,)* S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<$ty<$($p,)* S2, D2>>
            for $ty<$($o,)* S1, D1>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
            $($p: QuasiObserver<Target: Deref<Target: AsDeref<$p::InnerDepth>>>,)*
            D1: Unsigned,
            D2: Unsigned,
            S1: AsDeref<D1>,
            S2: AsDeref<D2>,
            S1::Target: PartialOrd<S2::Target>,
        {
            fn partial_cmp(&self, other: &$ty<$($p,)* S2, D2>) -> Option<std::cmp::Ordering> {
                self.untracked_ref().partial_cmp(other.untracked_ref())
            }
        }

        impl<$($o,)* S: ?Sized, D> Ord for $ty<$($o,)* S, D>
        where
            $($o: QuasiObserver<Target: Deref<Target: AsDeref<$o::InnerDepth>>>,)*
            D: Unsigned,
            S: AsDeref<D>,
            S::Target: Ord,
        {
            fn cmp(&self, other: &$ty<$($o,)* S, D>) -> std::cmp::Ordering {
                self.untracked_ref().cmp(other.untracked_ref())
            }
        }

        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($t,)*> Observe for ($($t,)*)
        where
            $($t: Observe,)*
        {
            type Observer<'ob, S, D>
                = $ty<$($t::Observer<'ob, $t, Zero>,)* S, D>
            where
                Self: 'ob,
                D: Unsigned,
                S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

            type Spec = DefaultSpec;
        }

        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($t,)*> RoObserve for ($($t,)*)
        where
            $($t: RoObserve,)*
        {
            type Observer<'ob, S, D>
                = $ty<$($t::Observer<'ob, $t, Zero>,)* S, D>
            where
                Self: 'ob,
                D: Unsigned,
                S: AsDeref<D, Target = Self> + ?Sized + 'ob;

            type Spec = DefaultSpec;
        }

        impl<$($t: Snapshot,)*> Snapshot for ($($t,)*) {
            type Snapshot = ($($t::Snapshot,)*);

            fn to_snapshot(&self) -> Self::Snapshot {
                ($(self.$n.to_snapshot(),)*)
            }
        }

        impl<$($t: SerializeSnapshot + 'static,)*> SerializeSnapshot for ($($t,)*) {
            fn flush(&self, snapshot: Self::Snapshot) -> Mutations {
                let mutations_tuple = ($(self.$n.flush(snapshot.$n).with_prefix($n),)*);
                let capacity = 0 $(+ mutations_tuple.$n.len())*;
                if capacity == $ptr {
                    return Mutations::replace(self);
                }
                let mut mutations = Mutations::new().with_capacity(capacity);
                $(mutations.extend(mutations_tuple.$n);)*
                mutations
            }
        }
    };
}

tuple_observer!(TupleObserver2; 2; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1);
tuple_observer!(TupleObserver3; 3; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2);
tuple_observer!(TupleObserver4; 4; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3);
tuple_observer!(TupleObserver5; 5; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3; O5, P5, T5, U5, 4);
tuple_observer!(TupleObserver6; 6; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3; O5, P5, T5, U5, 4; O6, P6, T6, U6, 5);
tuple_observer!(TupleObserver7; 7; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3; O5, P5, T5, U5, 4; O6, P6, T6, U6, 5; O7, P7, T7, U7, 6);
tuple_observer!(TupleObserver8; 8; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3; O5, P5, T5, U5, 4; O6, P6, T6, U6, 5; O7, P7, T7, U7, 6; O8, P8, T8, U8, 7);
tuple_observer!(TupleObserver9; 9; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3; O5, P5, T5, U5, 4; O6, P6, T6, U6, 5; O7, P7, T7, U7, 6; O8, P8, T8, U8, 7; O9, P9, T9, U9, 8);
tuple_observer!(TupleObserver10; 10; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3; O5, P5, T5, U5, 4; O6, P6, T6, U6, 5; O7, P7, T7, U7, 6; O8, P8, T8, U8, 7; O9, P9, T9, U9, 8; O10, P10, T10, U10, 9);
tuple_observer!(TupleObserver11; 11; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3; O5, P5, T5, U5, 4; O6, P6, T6, U6, 5; O7, P7, T7, U7, 6; O8, P8, T8, U8, 7; O9, P9, T9, U9, 8; O10, P10, T10, U10, 9; O11, P11, T11, U11, 10);
tuple_observer!(TupleObserver12; 12; O1, P1, T1, U1, 0; O2, P2, T2, U2, 1; O3, P3, T3, U3, 2; O4, P4, T4, U4, 3; O5, P5, T5, U5, 4; O6, P6, T6, U6, 5; O7, P7, T7, U7, 6; O8, P8, T8, U8, 7; O9, P9, T9, U9, 8; O10, P10, T10, U10, 9; O11, P11, T11, U11, 10; O12, P12, T12, U12, 11);

#[cfg(test)]
mod tests {
    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_change_returns_none() {
        let mut tuple = (String::from("hello"),);
        let mut ob = tuple.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn append_triggers_append() {
        let mut tuple = (String::from("hello"),);
        let mut ob = tuple.__observe();
        ob.0.push_str(" world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(0, json!(" world"))));
    }

    #[test]
    fn read_inner_after_deref_mut() {
        let mut tuple = (String::from("hello"),);
        let mut ob = tuple.__observe();
        *ob.tracked_mut() = (String::from("world"),);
        // Inner StringObserver's Pointer tag was killed by outer Unique retag.
        // Reading through the inner observer exercises that tag.
        let s: &String = ob.0.untracked_ref();
        assert_eq!(s.as_str(), "world");
    }

    #[test]
    fn deref_triggers_replace() {
        // Same-length replacement: inner StringObserver cannot detect this
        // because it only tracks length-based changes (append/truncate).
        let mut tuple = (String::from("hello"),);
        let mut ob = tuple.__observe();
        *ob.tracked_mut() = (String::from("world"),);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(["world"]))));

        // Longer replacement: without `as_deref_mut_coinductive`, inner
        // StringObserver would incorrectly produce Append(" world").
        let mut tuple = (String::from("hello"),);
        let mut ob = tuple.__observe();
        *ob.tracked_mut() = (String::from("hello world"),);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(["hello world"]))));
    }
}
