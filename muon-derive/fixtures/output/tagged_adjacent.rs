#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;
#[rustfmt::skip]
#[derive(Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Foo<'i> {
    A(u32),
    B(u32, u32),
    C { bar: &'i mut String },
}
#[rustfmt::skip]
const _: () = {
    pub struct FooObserver<'ob, 'i, S: ?Sized, N = ::muon::helper::Zero>
    where
        &'i mut String: ::muon::Observe + 'ob,
    {
        ptr: ::muon::helper::Pointer<S>,
        mutated: bool,
        variant: FooObserverVariant<'ob, 'i>,
        phantom: ::std::marker::PhantomData<&'ob mut N>,
    }
    pub enum FooObserverVariant<'ob, 'i>
    where
        &'i mut String: ::muon::Observe + 'ob,
    {
        A(::muon::observe::DefaultObserver<'ob, u32>),
        B(
            ::muon::observe::DefaultObserver<'ob, u32>,
            ::muon::observe::DefaultObserver<'ob, u32>,
        ),
        C { bar: ::muon::observe::DefaultObserver<'ob, &'i mut String> },
        __Unknown,
    }
    impl<'ob, 'i> FooObserverVariant<'ob, 'i>
    where
        &'i mut String: ::muon::Observe,
    {
        unsafe fn observe(__ptr: *mut Foo<'i>) -> Self {
            unsafe {
                match &*__ptr {
                    Foo::A(v0) => {
                        Self::A(
                            ::muon::observe::Observer::observe(
                                __ptr.with_addr(v0 as *const _ as usize).cast(),
                            ),
                        )
                    }
                    Foo::B(v0, v1) => {
                        Self::B(
                            ::muon::observe::Observer::observe(
                                __ptr.with_addr(v0 as *const _ as usize).cast(),
                            ),
                            ::muon::observe::Observer::observe(
                                __ptr.with_addr(v1 as *const _ as usize).cast(),
                            ),
                        )
                    }
                    Foo::C { bar } => {
                        Self::C {
                            bar: ::muon::observe::Observer::observe(
                                __ptr.with_addr(bar as *const _ as usize).cast(),
                            ),
                        }
                    }
                }
            }
        }
        unsafe fn relocate(&mut self, __ptr: *mut Foo<'i>) {
            unsafe {
                match (self, &*__ptr) {
                    (Self::A(u0), Foo::A(v0)) => {
                        ::muon::observe::Observer::relocate(
                            u0,
                            __ptr.with_addr(v0 as *const _ as usize).cast(),
                        );
                    }
                    (Self::B(u0, u1), Foo::B(v0, v1)) => {
                        ::muon::observe::Observer::relocate(
                            u0,
                            __ptr.with_addr(v0 as *const _ as usize).cast(),
                        );
                        ::muon::observe::Observer::relocate(
                            u1,
                            __ptr.with_addr(v1 as *const _ as usize).cast(),
                        );
                    }
                    (Self::C { bar: u0 }, Foo::C { bar: v0 }) => {
                        ::muon::observe::Observer::relocate(
                            u0,
                            __ptr.with_addr(v0 as *const _ as usize).cast(),
                        );
                    }
                    (Self::__Unknown, _) => {}
                    _ => panic!("inconsistent state for FooObserver"),
                }
            }
        }
        fn flush(&mut self, __ptr: *const Foo<'i>) -> ::muon::Mutations
        where
            Foo<'i>: ::muon::helper::serde::Serialize + 'static,
            ::muon::observe::DefaultObserver<
                'ob,
                &'i mut String,
            >: ::muon::observe::SerializeObserver,
        {
            match self {
                Self::A(u0) => {
                    ::muon::observe::SerializeObserver::flush(u0).with_prefix("data")
                }
                Self::B(u0, u1) => {
                    let mutations_0 = ::muon::observe::SerializeObserver::flush(u0);
                    let mutations_1 = ::muon::observe::SerializeObserver::flush(u1);
                    if mutations_0.is_replace() && mutations_1.is_replace() {
                        return ::muon::Mutations::replace(unsafe { &*__ptr });
                    }
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(
                            !mutations_0.is_empty() as usize
                                + !mutations_1.is_empty() as usize,
                        );
                    mutations.insert(0usize, mutations_0);
                    mutations.insert(1usize, mutations_1);
                    mutations.with_prefix("data")
                }
                Self::C { bar } => {
                    let mutations_bar = ::muon::observe::SerializeObserver::flush(bar);
                    if mutations_bar.is_replace() {
                        return ::muon::Mutations::replace(unsafe { &*__ptr });
                    }
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(!mutations_bar.is_empty() as usize);
                    mutations.insert("bar", mutations_bar);
                    mutations.with_prefix("data")
                }
                Self::__Unknown => ::muon::Mutations::new(),
            }
        }
        fn flat_flush(&mut self, __ptr: *const Foo<'i>) -> ::muon::Mutations
        where
            Foo<'i>: ::muon::helper::serde::Serialize + 'static,
            ::muon::observe::DefaultObserver<
                'ob,
                &'i mut String,
            >: ::muon::observe::SerializeObserver,
        {
            match self {
                Self::A(u0) => {
                    ::muon::observe::SerializeObserver::flat_flush(u0)
                        .with_prefix("data")
                }
                Self::C { bar } => {
                    let mutations_bar = ::muon::observe::SerializeObserver::flush(bar);
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(!mutations_bar.is_empty() as usize)
                        .with_replace(mutations_bar.is_replace());
                    mutations.insert("bar", mutations_bar);
                    mutations.with_prefix("data")
                }
                _ => panic!("flat_flush can only be called on structs and maps"),
            }
        }
    }
    #[automatically_derived]
    impl<'ob, 'i, S: ?Sized, N> ::std::ops::Deref for FooObserver<'ob, 'i, S, N>
    where
        &'i mut String: ::muon::Observe,
    {
        type Target = ::muon::helper::Pointer<S>;
        fn deref(&self) -> &Self::Target {
            &self.ptr
        }
    }
    #[automatically_derived]
    impl<'ob, 'i, S: ?Sized, N> ::std::ops::DerefMut for FooObserver<'ob, 'i, S, N>
    where
        &'i mut String: ::muon::Observe,
    {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.mutated = true;
            self.variant = FooObserverVariant::__Unknown;
            &mut self.ptr
        }
    }
    #[automatically_derived]
    impl<'ob, 'i, S: ?Sized, N> ::muon::helper::QuasiObserver
    for FooObserver<'ob, 'i, S, N>
    where
        &'i mut String: ::muon::Observe,
        S: ::muon::helper::AsDeref<N>,
        N: ::muon::helper::Unsigned,
    {
        type Head = S;
        type OuterDepth = ::muon::helper::Succ<::muon::helper::Zero>;
        type InnerDepth = N;
        fn invalidate(this: &mut Self) {
            this.mutated = true;
            this.variant = FooObserverVariant::__Unknown;
        }
    }
    #[automatically_derived]
    impl<'ob, 'i, S: ?Sized, N> ::muon::observe::Observer for FooObserver<'ob, 'i, S, N>
    where
        &'i mut String: ::muon::Observe,
        S: ::muon::helper::AsDeref<N, Target = Foo<'i>>,
        N: ::muon::helper::Unsigned,
    {
        unsafe fn observe(head: *mut S) -> Self {
            unsafe {
                let __ptr = ::muon::helper::AsDerefPtrExt::as_deref_ptr::<N>(head);
                Self {
                    mutated: false,
                    variant: FooObserverVariant::observe(__ptr),
                    ptr: ::muon::helper::Pointer::new_unchecked(head),
                    phantom: ::std::marker::PhantomData,
                }
            }
        }
        unsafe fn relocate(this: &mut Self, head: *mut S) {
            let __ptr = unsafe {
                ::muon::helper::AsDerefPtrExt::as_deref_ptr::<N>(head)
            };
            unsafe { this.variant.relocate(__ptr) }
            unsafe { ::muon::helper::Pointer::set_unchecked(this, head) };
        }
    }
    #[automatically_derived]
    impl<'ob, 'i, S: ?Sized, N> ::muon::observe::SerializeObserver
    for FooObserver<'ob, 'i, S, N>
    where
        Foo<'i>: ::muon::helper::serde::Serialize + 'static,
        &'i mut String: ::muon::Observe,
        S: ::muon::helper::AsDeref<N, Target = Foo<'i>>,
        N: ::muon::helper::Unsigned,
        ::muon::observe::DefaultObserver<
            'ob,
            &'i mut String,
        >: ::muon::observe::SerializeObserver,
    {
        fn flush(this: &mut Self) -> ::muon::Mutations {
            let value = this.ptr.as_deref();
            if !this.mutated {
                return this.variant.flush(value);
            }
            this.mutated = false;
            this.variant = FooObserverVariant::__Unknown;
            ::muon::Mutations::replace(this.as_deref())
        }
        fn flat_flush(this: &mut Self) -> ::muon::Mutations {
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
    impl<'i> ::muon::Observe for Foo<'i>
    where
        Self: ::muon::helper::serde::Serialize,
        &'i mut String: ::muon::Observe,
    {
        type Observer<'ob, S, N> = FooObserver<'ob, 'i, S, N>
        where
            Self: 'ob,
            &'i mut String: 'ob,
            N: ::muon::helper::Unsigned,
            S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
        type Spec = ::muon::observe::DefaultSpec;
    }
};
