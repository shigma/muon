#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;
#[rustfmt::skip]
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Foo<S, T, U, V>
where
    T: Clone,
{
    A(S),
    B(u32, U, V),
    #[serde(rename_all = "UPPERCASE")]
    #[serde(rename = "OwO")]
    C { #[serde(skip)] bar: Option<T>, #[serde(rename = "QwQ")] qux: Qux },
    D,
    E(),
    F {},
}
#[rustfmt::skip]
const _: () = {
    pub struct FooObserver<'ob, S, T, U, V, _S: ?Sized, N = ::muon::helper::Zero>
    where
        T: Clone,
        U: ::muon::Observe + 'ob,
        V: ::muon::general::Snapshot,
    {
        ptr: ::muon::helper::Pointer<_S>,
        mutated: bool,
        initial: FooObserverInitial,
        variant: FooObserverVariant<'ob, S, T, U, V>,
        phantom: ::std::marker::PhantomData<&'ob mut N>,
    }
    #[derive(Clone, Copy)]
    #[allow(clippy::enum_variant_names)]
    pub enum FooObserverInitial {
        D,
        E,
        F,
        __Unknown,
    }
    impl FooObserverInitial {
        fn new<S, T, U, V>(value: &Foo<S, T, U, V>) -> Self
        where
            T: Clone,
        {
            match value {
                Foo::D => FooObserverInitial::D,
                Foo::E() => FooObserverInitial::E,
                Foo::F {} => FooObserverInitial::F,
                _ => FooObserverInitial::__Unknown,
            }
        }
    }
    pub enum FooObserverVariant<'ob, S, T, U, V>
    where
        T: Clone,
        U: ::muon::Observe + 'ob,
        V: ::muon::general::Snapshot,
    {
        A(::muon::helper::Pointer<S>),
        B(
            ::muon::observe::DefaultObserver<'ob, u32>,
            ::muon::observe::DefaultObserver<'ob, U>,
            ::muon::general::SnapshotObserver<'ob, V, V>,
        ),
        C {
            bar: ::muon::helper::Pointer<Option<T>>,
            qux: ::muon::observe::DefaultObserver<'ob, Qux>,
        },
        __Unknown,
    }
    impl<'ob, S, T, U, V> FooObserverVariant<'ob, S, T, U, V>
    where
        T: Clone,
        U: ::muon::Observe,
        V: ::muon::general::Snapshot,
    {
        unsafe fn observe(__ptr: *mut Foo<S, T, U, V>) -> Self {
            unsafe {
                match &*__ptr {
                    Foo::A(v0) => {
                        Self::A(
                            ::muon::helper::Pointer::new_unchecked(
                                __ptr.with_addr(v0 as *const _ as usize).cast(),
                            ),
                        )
                    }
                    Foo::B(v0, v1, v2) => {
                        Self::B(
                            ::muon::observe::Observer::observe(
                                __ptr.with_addr(v0 as *const _ as usize).cast(),
                            ),
                            ::muon::observe::Observer::observe(
                                __ptr.with_addr(v1 as *const _ as usize).cast(),
                            ),
                            ::muon::observe::Observer::observe(
                                __ptr.with_addr(v2 as *const _ as usize).cast(),
                            ),
                        )
                    }
                    Foo::C { bar, qux } => {
                        Self::C {
                            bar: ::muon::helper::Pointer::new_unchecked(
                                __ptr.with_addr(bar as *const _ as usize).cast(),
                            ),
                            qux: ::muon::observe::Observer::observe(
                                __ptr.with_addr(qux as *const _ as usize).cast(),
                            ),
                        }
                    }
                    _ => Self::__Unknown,
                }
            }
        }
        unsafe fn relocate(&mut self, __ptr: *mut Foo<S, T, U, V>) {
            unsafe {
                match (self, &*__ptr) {
                    (Self::A(u0), Foo::A(v0)) => {
                        ::muon::helper::Pointer::set(u0, v0);
                    }
                    (Self::B(u0, u1, u2), Foo::B(v0, v1, v2)) => {
                        ::muon::observe::Observer::relocate(
                            u0,
                            __ptr.with_addr(v0 as *const _ as usize).cast(),
                        );
                        ::muon::observe::Observer::relocate(
                            u1,
                            __ptr.with_addr(v1 as *const _ as usize).cast(),
                        );
                        ::muon::observe::Observer::relocate(
                            u2,
                            __ptr.with_addr(v2 as *const _ as usize).cast(),
                        );
                    }
                    (Self::C { bar: u0, qux: u1 }, Foo::C { bar: v0, qux: v1 }) => {
                        ::muon::helper::Pointer::set(u0, v0);
                        ::muon::observe::Observer::relocate(
                            u1,
                            __ptr.with_addr(v1 as *const _ as usize).cast(),
                        );
                    }
                    (Self::__Unknown, _) => {}
                    _ => panic!("inconsistent state for FooObserver"),
                }
            }
        }
        fn flush(&mut self, __ptr: *const Foo<S, T, U, V>) -> ::muon::Mutations
        where
            Foo<S, T, U, V>: ::muon::helper::serde::Serialize + 'static,
            ::muon::observe::DefaultObserver<'ob, U>: ::muon::observe::SerializeObserver,
            ::muon::general::SnapshotObserver<
                'ob,
                V,
                V,
            >: ::muon::observe::SerializeObserver,
        {
            match self {
                Self::A(_) => ::muon::Mutations::new(),
                Self::B(u0, u1, u2) => {
                    let mutations_0 = ::muon::observe::SerializeObserver::flush(u0);
                    let mutations_1 = ::muon::observe::SerializeObserver::flush(u1);
                    let mutations_2 = ::muon::observe::SerializeObserver::flush(u2);
                    if mutations_0.is_replace() && mutations_1.is_replace()
                        && mutations_2.is_replace()
                    {
                        return ::muon::Mutations::replace(unsafe { &*__ptr });
                    }
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(
                            !mutations_0.is_empty() as usize
                                + !mutations_1.is_empty() as usize
                                + !mutations_2.is_empty() as usize,
                        );
                    mutations.insert(0usize, mutations_0);
                    mutations.insert(1usize, mutations_1);
                    mutations.insert(2usize, mutations_2);
                    mutations.with_prefix("b")
                }
                Self::C { qux, .. } => {
                    let mutations_qux = ::muon::observe::SerializeObserver::flush(qux);
                    if mutations_qux.is_replace() {
                        return ::muon::Mutations::replace(unsafe { &*__ptr });
                    }
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(!mutations_qux.is_empty() as usize);
                    mutations.insert("QwQ", mutations_qux);
                    mutations.with_prefix("OwO")
                }
                Self::__Unknown => ::muon::Mutations::new(),
            }
        }
        fn flat_flush(&mut self, __ptr: *const Foo<S, T, U, V>) -> ::muon::Mutations
        where
            Foo<S, T, U, V>: ::muon::helper::serde::Serialize + 'static,
            ::muon::observe::DefaultObserver<'ob, U>: ::muon::observe::SerializeObserver,
            ::muon::general::SnapshotObserver<
                'ob,
                V,
                V,
            >: ::muon::observe::SerializeObserver,
        {
            match self {
                Self::A(_) => ::muon::Mutations::new(),
                Self::C { qux, .. } => {
                    let mutations_qux = ::muon::observe::SerializeObserver::flush(qux);
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(!mutations_qux.is_empty() as usize)
                        .with_replace(mutations_qux.is_replace());
                    mutations.insert("QwQ", mutations_qux);
                    mutations.with_prefix("OwO")
                }
                _ => panic!("flat_flush can only be called on structs and maps"),
            }
        }
    }
    #[automatically_derived]
    impl<'ob, S, T, U, V, _S: ?Sized, N> ::std::ops::Deref
    for FooObserver<'ob, S, T, U, V, _S, N>
    where
        T: Clone,
        U: ::muon::Observe,
        V: ::muon::general::Snapshot,
    {
        type Target = ::muon::helper::Pointer<_S>;
        fn deref(&self) -> &Self::Target {
            &self.ptr
        }
    }
    #[automatically_derived]
    impl<'ob, S, T, U, V, _S: ?Sized, N> ::std::ops::DerefMut
    for FooObserver<'ob, S, T, U, V, _S, N>
    where
        T: Clone,
        U: ::muon::Observe,
        V: ::muon::general::Snapshot,
    {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.mutated = true;
            self.variant = FooObserverVariant::__Unknown;
            &mut self.ptr
        }
    }
    #[automatically_derived]
    impl<'ob, S, T, U, V, _S: ?Sized, N> ::muon::helper::QuasiObserver
    for FooObserver<'ob, S, T, U, V, _S, N>
    where
        T: Clone,
        U: ::muon::Observe,
        V: ::muon::general::Snapshot,
        _S: ::muon::helper::AsDeref<N>,
        N: ::muon::helper::Unsigned,
    {
        type Head = _S;
        type OuterDepth = ::muon::helper::Succ<::muon::helper::Zero>;
        type InnerDepth = N;
        fn invalidate(this: &mut Self) {
            this.mutated = true;
            this.variant = FooObserverVariant::__Unknown;
        }
    }
    #[automatically_derived]
    impl<'ob, S, T, U, V, _S: ?Sized, N> ::muon::observe::Observer
    for FooObserver<'ob, S, T, U, V, _S, N>
    where
        T: Clone,
        S: 'ob,
        V: 'ob,
        Option<T>: 'ob,
        U: ::muon::Observe,
        V: ::muon::general::Snapshot,
        _S: ::muon::helper::AsDeref<N, Target = Foo<S, T, U, V>>,
        N: ::muon::helper::Unsigned,
    {
        unsafe fn observe(head: *mut _S) -> Self {
            unsafe {
                let __ptr = ::muon::helper::AsDerefPtrExt::as_deref_ptr::<N>(head);
                Self {
                    mutated: false,
                    initial: FooObserverInitial::new(&*__ptr),
                    variant: FooObserverVariant::observe(__ptr),
                    ptr: ::muon::helper::Pointer::new_unchecked(head),
                    phantom: ::std::marker::PhantomData,
                }
            }
        }
        unsafe fn relocate(this: &mut Self, head: *mut _S) {
            let __ptr = unsafe {
                ::muon::helper::AsDerefPtrExt::as_deref_ptr::<N>(head)
            };
            unsafe { this.variant.relocate(__ptr) }
            unsafe { ::muon::helper::Pointer::set_unchecked(this, head) };
        }
    }
    #[automatically_derived]
    impl<'ob, S, T, U, V, _S: ?Sized, N> ::muon::observe::SerializeObserver
    for FooObserver<'ob, S, T, U, V, _S, N>
    where
        Foo<S, T, U, V>: ::muon::helper::serde::Serialize + 'static,
        T: Clone,
        S: 'ob,
        V: 'ob,
        Option<T>: 'ob,
        U: ::muon::Observe,
        V: ::muon::general::Snapshot,
        _S: ::muon::helper::AsDeref<N, Target = Foo<S, T, U, V>>,
        N: ::muon::helper::Unsigned,
        ::muon::observe::DefaultObserver<'ob, U>: ::muon::observe::SerializeObserver,
        ::muon::general::SnapshotObserver<'ob, V, V>: ::muon::observe::SerializeObserver,
    {
        fn flush(this: &mut Self) -> ::muon::Mutations {
            let value = this.ptr.as_deref();
            let initial = this.initial;
            this.initial = FooObserverInitial::new(value);
            if !this.mutated {
                return this.variant.flush(value);
            }
            this.mutated = false;
            this.variant = FooObserverVariant::__Unknown;
            match (initial, value) {
                (FooObserverInitial::D, Foo::D)
                | (FooObserverInitial::E, Foo::E())
                | (FooObserverInitial::F, Foo::F {}) => ::muon::Mutations::new(),
                _ => ::muon::Mutations::replace(value),
            }
        }
        fn flat_flush(this: &mut Self) -> ::muon::Mutations {
            let value = this.ptr.as_deref();
            let initial = this.initial;
            this.initial = FooObserverInitial::new(value);
            if !this.mutated {
                return this.variant.flat_flush(value);
            }
            this.mutated = false;
            this.variant = FooObserverVariant::__Unknown;
            match (initial, value) {
                (FooObserverInitial::D, Foo::D)
                | (FooObserverInitial::E, Foo::E())
                | (FooObserverInitial::F, Foo::F {}) => ::muon::Mutations::new(),
                _ => ::muon::Mutations::replace(value),
            }
        }
    }
    #[automatically_derived]
    impl<S, T, U, V> ::muon::Observe for Foo<S, T, U, V>
    where
        Self: ::muon::helper::serde::Serialize,
        T: Clone,
        U: ::muon::Observe,
        V: ::muon::general::Snapshot,
    {
        type Observer<'ob, _S, N> = FooObserver<'ob, S, T, U, V, _S, N>
        where
            Self: 'ob,
            U: 'ob,
            N: ::muon::helper::Unsigned,
            _S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
        type Spec = ::muon::observe::DefaultSpec;
    }
};
#[rustfmt::skip]
#[derive(Serialize)]
pub struct Qux {}
#[rustfmt::skip]
#[automatically_derived]
impl ::muon::general::Snapshot for Qux {
    type Snapshot = ();
    fn to_snapshot(&self) {}
}
#[rustfmt::skip]
#[automatically_derived]
impl ::muon::general::SerializeSnapshot for Qux {
    fn flush(&self, _snapshot: ()) -> ::muon::Mutations {
        ::muon::Mutations::new()
    }
}
#[rustfmt::skip]
#[automatically_derived]
impl ::muon::Observe for Qux {
    type Observer<'ob, S, N> = ::muon::general::NoopObserver<'ob, Self, S, N>
    where
        Self: 'ob,
        N: ::muon::helper::Unsigned,
        S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
    type Spec = ::muon::observe::SnapshotSpec;
}
