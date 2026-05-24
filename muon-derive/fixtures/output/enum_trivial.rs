#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;
#[rustfmt::skip]
#[derive(PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Foo {
    A,
    B(),
    C {},
}
#[rustfmt::skip]
const _: () = {
    #[::std::prelude::v1::derive()]
    pub struct FooObserver<'ob, S: ?Sized, N = ::muon::helper::Zero> {
        ptr: ::muon::helper::Pointer<S>,
        initial: FooObserverInitial,
        phantom: ::std::marker::PhantomData<&'ob mut N>,
    }
    #[derive(Clone, Copy)]
    #[allow(clippy::enum_variant_names)]
    pub enum FooObserverInitial {
        A,
        B,
        C,
    }
    impl FooObserverInitial {
        fn new(value: &Foo) -> Self {
            match value {
                Foo::A => FooObserverInitial::A,
                Foo::B() => FooObserverInitial::B,
                Foo::C {} => FooObserverInitial::C,
            }
        }
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::std::ops::Deref for FooObserver<'ob, S, N> {
        type Target = ::muon::helper::Pointer<S>;
        fn deref(&self) -> &Self::Target {
            &self.ptr
        }
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::std::ops::DerefMut for FooObserver<'ob, S, N> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.ptr
        }
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::muon::helper::QuasiObserver for FooObserver<'ob, S, N>
    where
        S: ::muon::helper::AsDeref<N>,
        N: ::muon::helper::Unsigned,
    {
        type Head = S;
        type OuterDepth = ::muon::helper::Succ<::muon::helper::Zero>;
        type InnerDepth = N;
        fn invalidate(this: &mut Self) {}
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::muon::observe::Observer for FooObserver<'ob, S, N>
    where
        S: ::muon::helper::AsDerefMut<N, Target = Foo>,
        N: ::muon::helper::Unsigned,
    {
        fn observe(head: &mut S) -> Self {
            let __value = head.as_deref_mut();
            Self {
                initial: FooObserverInitial::new(__value),
                ptr: ::muon::helper::Pointer::new(head),
                phantom: ::std::marker::PhantomData,
            }
        }
        unsafe fn relocate(this: &mut Self, head: &mut S) {
            ::muon::helper::Pointer::set(this, head);
        }
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::muon::observe::SerializeObserver for FooObserver<'ob, S, N>
    where
        S: ::muon::helper::AsDeref<N, Target = Foo>,
        N: ::muon::helper::Unsigned,
    {
        unsafe fn flush(this: &mut Self) -> ::muon::Mutations {
            let value = this.ptr.as_deref();
            let initial = this.initial;
            this.initial = FooObserverInitial::new(value);
            match (initial, value) {
                (FooObserverInitial::A, Foo::A)
                | (FooObserverInitial::B, Foo::B())
                | (FooObserverInitial::C, Foo::C {}) => ::muon::Mutations::new(),
                _ => ::muon::Mutations::replace(value),
            }
        }
        unsafe fn flat_flush(this: &mut Self) -> ::muon::Mutations {
            let value = this.ptr.as_deref();
            let initial = this.initial;
            this.initial = FooObserverInitial::new(value);
            match (initial, value) {
                (FooObserverInitial::A, Foo::A)
                | (FooObserverInitial::B, Foo::B())
                | (FooObserverInitial::C, Foo::C {}) => ::muon::Mutations::new(),
                _ => ::muon::Mutations::replace(value),
            }
        }
    }
    #[automatically_derived]
    impl ::muon::Observe for Foo {
        type Observer<'ob, S, N> = FooObserver<'ob, S, N>
        where
            Self: 'ob,
            N: ::muon::helper::Unsigned,
            S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
        type Spec = ::muon::observe::DefaultSpec;
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::std::cmp::PartialEq for FooObserver<'ob, S, N>
    where
        S: ::muon::helper::AsDeref<N, Target = Foo>,
        N: ::muon::helper::Unsigned,
    {
        fn eq(&self, other: &Self) -> bool {
            self.as_deref().eq(other.as_deref())
        }
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::std::cmp::Eq for FooObserver<'ob, S, N>
    where
        S: ::muon::helper::AsDeref<N, Target = Foo>,
        N: ::muon::helper::Unsigned,
    {}
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::std::cmp::PartialOrd for FooObserver<'ob, S, N>
    where
        S: ::muon::helper::AsDeref<N, Target = Foo>,
        N: ::muon::helper::Unsigned,
    {
        fn partial_cmp(
            &self,
            other: &Self,
        ) -> ::std::option::Option<::std::cmp::Ordering> {
            self.as_deref().partial_cmp(other.as_deref())
        }
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::std::cmp::Ord for FooObserver<'ob, S, N>
    where
        S: ::muon::helper::AsDeref<N, Target = Foo>,
        N: ::muon::helper::Unsigned,
    {
        fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
            self.as_deref().cmp(other.as_deref())
        }
    }
};
#[rustfmt::skip]
#[derive(Serialize)]
pub enum Bar {
    A,
    B(),
    C {},
}
#[rustfmt::skip]
const _: () = {
    pub enum BarSnapshot {
        A,
        B(),
        C {},
    }
    #[automatically_derived]
    impl ::muon::general::Snapshot for Bar {
        type Snapshot = BarSnapshot;
        fn to_snapshot(&self) -> Self::Snapshot {
            match self {
                Self::A => BarSnapshot::A,
                Self::B() => BarSnapshot::B(),
                Self::C {} => BarSnapshot::C {},
            }
        }
        #[allow(clippy::match_like_matches_macro)]
        fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
            match (self, snapshot) {
                (Self::A, BarSnapshot::A) => true,
                (Self::B(), BarSnapshot::B()) => true,
                (Self::C {}, BarSnapshot::C {}) => true,
                _ => false,
            }
        }
    }
};
#[rustfmt::skip]
#[automatically_derived]
impl ::muon::Observe for Bar
where
    Self: ::muon::general::Snapshot,
{
    type Observer<'ob, S, N> = ::muon::general::SnapshotObserver<'ob, S, N>
    where
        Self: 'ob,
        N: ::muon::helper::Unsigned,
        S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
    type Spec = ::muon::observe::SnapshotSpec;
}
