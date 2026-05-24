#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;
#[rustfmt::skip]
#[derive(Serialize)]
#[serde(tag = "type")]
pub enum Foo<const N: usize> {
    #[serde(serialize_with = "<[_]>::serialize")]
    A([u32; N]),
    C {
        #[serde(skip_serializing_if = "String::is_empty")]
        bar: String,
        #[serde(flatten)]
        qux: Qux,
    },
}
#[rustfmt::skip]
const _: () = {
    pub struct FooObserver<'ob, const N: usize, S: ?Sized, _N = ::muon::helper::Zero> {
        ptr: ::muon::helper::Pointer<S>,
        mutated: bool,
        variant: FooObserverVariant<'ob, N>,
        phantom: ::std::marker::PhantomData<&'ob mut _N>,
    }
    pub enum FooObserverVariant<'ob, const N: usize> {
        A(::muon::observe::DefaultObserver<'ob, [u32; N]>),
        C {
            bar: ::muon::observe::DefaultObserver<'ob, String>,
            qux: ::muon::observe::DefaultObserver<'ob, Qux>,
        },
        __Unknown,
    }
    impl<'ob, const N: usize> FooObserverVariant<'ob, N> {
        fn observe(value: &mut Foo<N>) -> Self {
            match value {
                Foo::A(v0) => Self::A(::muon::observe::Observer::observe(v0)),
                Foo::C { bar, qux } => {
                    Self::C {
                        bar: ::muon::observe::Observer::observe(bar),
                        qux: ::muon::observe::Observer::observe(qux),
                    }
                }
            }
        }
        unsafe fn relocate(&mut self, __ptr: *mut Foo<N>) {
            unsafe {
                match (self, &*__ptr) {
                    (Self::A(u0), Foo::A(v0)) => {
                        ::muon::observe::Observer::relocate(
                            u0,
                            __ptr.with_addr(v0 as *const _ as usize).cast(),
                        );
                    }
                    (Self::C { bar: u0, qux: u1 }, Foo::C { bar: v0, qux: v1 }) => {
                        ::muon::observe::Observer::relocate(
                            u0,
                            __ptr.with_addr(v0 as *const _ as usize).cast(),
                        );
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
        fn flush(&mut self, __ptr: *const Foo<N>) -> ::muon::Mutations
        where
            Foo<N>: ::muon::helper::serde::Serialize + 'static,
        {
            match self {
                Self::A(u0) => unsafe { ::muon::observe::SerializeObserver::flush(u0) }
                Self::C { bar, qux } => {
                    let mutations_bar = unsafe {
                        ::muon::observe::SerializeObserver::flush(bar)
                    };
                    let mutations_qux = unsafe {
                        ::muon::observe::SerializeObserver::flat_flush(qux)
                    };
                    if mutations_bar.is_replace() && mutations_qux.is_replace() {
                        return ::muon::Mutations::replace(unsafe { &*__ptr });
                    }
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(
                            !mutations_bar.is_empty() as usize + mutations_qux.len(),
                        );
                    if !mutations_bar.is_empty()
                        && String::is_empty(
                            ::muon::helper::QuasiObserver::untracked_ref(bar),
                        )
                    {
                        mutations.insert("bar", ::muon::Mutations::delete());
                    } else {
                        mutations.insert("bar", mutations_bar);
                    }
                    mutations.extend(mutations_qux);
                    mutations
                }
                Self::__Unknown => ::muon::Mutations::new(),
            }
        }
        fn flat_flush(&mut self, __ptr: *const Foo<N>) -> ::muon::Mutations
        where
            Foo<N>: ::muon::helper::serde::Serialize + 'static,
        {
            match self {
                Self::A(u0) => {
                    unsafe { ::muon::observe::SerializeObserver::flat_flush(u0) }
                }
                Self::C { bar, qux } => {
                    let mutations_bar = unsafe {
                        ::muon::observe::SerializeObserver::flush(bar)
                    };
                    let mutations_qux = unsafe {
                        ::muon::observe::SerializeObserver::flat_flush(qux)
                    };
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(
                            !mutations_bar.is_empty() as usize + mutations_qux.len(),
                        )
                        .with_replace(
                            mutations_bar.is_replace() && mutations_qux.is_replace(),
                        );
                    if !mutations_bar.is_empty()
                        && String::is_empty(
                            ::muon::helper::QuasiObserver::untracked_ref(bar),
                        )
                    {
                        mutations.insert("bar", ::muon::Mutations::delete());
                    } else {
                        mutations.insert("bar", mutations_bar);
                    }
                    mutations.extend(mutations_qux);
                    mutations
                }
                _ => panic!("flat_flush can only be called on structs and maps"),
            }
        }
    }
    #[automatically_derived]
    impl<'ob, const N: usize, S: ?Sized, _N> ::std::ops::Deref
    for FooObserver<'ob, N, S, _N> {
        type Target = ::muon::helper::Pointer<S>;
        fn deref(&self) -> &Self::Target {
            &self.ptr
        }
    }
    #[automatically_derived]
    impl<'ob, const N: usize, S: ?Sized, _N> ::std::ops::DerefMut
    for FooObserver<'ob, N, S, _N> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.mutated = true;
            self.variant = FooObserverVariant::__Unknown;
            &mut self.ptr
        }
    }
    #[automatically_derived]
    impl<'ob, const N: usize, S: ?Sized, _N> ::muon::helper::QuasiObserver
    for FooObserver<'ob, N, S, _N>
    where
        S: ::muon::helper::AsDeref<_N>,
        _N: ::muon::helper::Unsigned,
    {
        type Head = S;
        type OuterDepth = ::muon::helper::Succ<::muon::helper::Zero>;
        type InnerDepth = _N;
        fn invalidate(this: &mut Self) {
            this.mutated = true;
            this.variant = FooObserverVariant::__Unknown;
        }
    }
    #[automatically_derived]
    impl<'ob, const N: usize, S: ?Sized, _N> ::muon::observe::Observer
    for FooObserver<'ob, N, S, _N>
    where
        S: ::muon::helper::AsDerefMut<_N, Target = Foo<N>>,
        _N: ::muon::helper::Unsigned,
    {
        fn observe(head: &mut S) -> Self {
            let __value = head.as_deref_mut();
            Self {
                mutated: false,
                variant: FooObserverVariant::observe(__value),
                ptr: ::muon::helper::Pointer::new(head),
                phantom: ::std::marker::PhantomData,
            }
        }
        unsafe fn relocate(this: &mut Self, head: *mut S) {
            let __ptr = unsafe {
                ::muon::helper::AsDerefPtrExt::as_deref_ptr::<_N>(head)
            };
            unsafe { this.variant.relocate(__ptr) }
            unsafe { ::muon::helper::Pointer::set_unchecked(this, head) };
        }
    }
    #[automatically_derived]
    impl<'ob, const N: usize, S: ?Sized, _N> ::muon::observe::SerializeObserver
    for FooObserver<'ob, N, S, _N>
    where
        Foo<N>: ::muon::helper::serde::Serialize + 'static,
        S: ::muon::helper::AsDeref<_N, Target = Foo<N>>,
        _N: ::muon::helper::Unsigned,
    {
        unsafe fn flush(this: &mut Self) -> ::muon::Mutations {
            let value = this.ptr.as_deref();
            if !this.mutated {
                return this.variant.flush(value);
            }
            this.mutated = false;
            this.variant = FooObserverVariant::__Unknown;
            ::muon::Mutations::replace(this.as_deref())
        }
        unsafe fn flat_flush(this: &mut Self) -> ::muon::Mutations {
            let value = this.ptr.as_deref();
            if !this.mutated {
                return this.variant.flat_flush(value);
            }
            this.mutated = false;
            this.variant = FooObserverVariant::__Unknown;
            ::muon::Mutations::replace(this.as_deref())
        }
    }
    #[automatically_derived]
    impl<const N: usize> ::muon::Observe for Foo<N>
    where
        Self: ::muon::helper::serde::Serialize,
    {
        type Observer<'ob, S, _N> = FooObserver<'ob, N, S, _N>
        where
            Self: 'ob,
            _N: ::muon::helper::Unsigned,
            S: ::muon::helper::AsDerefMut<_N, Target = Self> + ?Sized + 'ob;
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
    fn eq_snapshot(&self, snapshot: &()) -> bool {
        true
    }
}
#[rustfmt::skip]
#[automatically_derived]
impl ::muon::Observe for Qux {
    type Observer<'ob, S, N> = ::muon::general::NoopObserver<'ob, S, N>
    where
        Self: 'ob,
        N: ::muon::helper::Unsigned,
        S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
    type Spec = ::muon::observe::SnapshotSpec;
}
