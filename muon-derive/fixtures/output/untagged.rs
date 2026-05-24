use ::std::fmt::Display;
#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;
#[rustfmt::skip]
#[derive(Serialize)]
#[serde(untagged, rename_all_fields = "UPPERCASE")]
pub enum Foo {
    A(u32),
    B(u32, u32),
    C { bar: String },
    D,
    E(),
    F {},
}
#[rustfmt::skip]
const _: () = {
    #[::std::prelude::v1::derive()]
    pub struct FooObserver<'ob, S: ?Sized, N = ::muon::helper::Zero> {
        ptr: ::muon::helper::Pointer<S>,
        mutated: bool,
        initial: FooObserverInitial,
        variant: FooObserverVariant<'ob>,
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
        fn new(value: &Foo) -> Self {
            match value {
                Foo::D => FooObserverInitial::D,
                Foo::E() => FooObserverInitial::E,
                Foo::F {} => FooObserverInitial::F,
                _ => FooObserverInitial::__Unknown,
            }
        }
    }
    pub enum FooObserverVariant<'ob> {
        A(::muon::observe::DefaultObserver<'ob, u32>),
        B(
            ::muon::observe::DefaultObserver<'ob, u32>,
            ::muon::observe::DefaultObserver<'ob, u32>,
        ),
        C { bar: ::muon::observe::DefaultObserver<'ob, String> },
        __Unknown,
    }
    impl<'ob> FooObserverVariant<'ob> {
        fn observe(value: &mut Foo) -> Self {
            match value {
                Foo::A(v0) => Self::A(::muon::observe::Observer::observe(v0)),
                Foo::B(v0, v1) => {
                    Self::B(
                        ::muon::observe::Observer::observe(v0),
                        ::muon::observe::Observer::observe(v1),
                    )
                }
                Foo::C { bar } => {
                    Self::C {
                        bar: ::muon::observe::Observer::observe(bar),
                    }
                }
                _ => Self::__Unknown,
            }
        }
        unsafe fn relocate(&mut self, value: &mut Foo) {
            unsafe {
                match (self, value) {
                    (Self::A(u0), Foo::A(v0)) => {
                        ::muon::observe::Observer::relocate(u0, v0);
                    }
                    (Self::B(u0, u1), Foo::B(v0, v1)) => {
                        ::muon::observe::Observer::relocate(u0, v0);
                        ::muon::observe::Observer::relocate(u1, v1);
                    }
                    (Self::C { bar: u0 }, Foo::C { bar: v0 }) => {
                        ::muon::observe::Observer::relocate(u0, v0);
                    }
                    (Self::__Unknown, _) => {}
                    _ => panic!("inconsistent state for FooObserver"),
                }
            }
        }
        fn flush(&mut self, __value: *const Foo) -> ::muon::Mutations {
            match self {
                Self::A(u0) => unsafe { ::muon::observe::SerializeObserver::flush(u0) }
                Self::B(u0, u1) => {
                    let mutations_0 = unsafe {
                        ::muon::observe::SerializeObserver::flush(u0)
                    };
                    let mutations_1 = unsafe {
                        ::muon::observe::SerializeObserver::flush(u1)
                    };
                    if mutations_0.is_replace() && mutations_1.is_replace() {
                        return ::muon::Mutations::replace(unsafe { &*__value });
                    }
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(
                            !mutations_0.is_empty() as usize
                                + !mutations_1.is_empty() as usize,
                        );
                    mutations.insert(0usize, mutations_0);
                    mutations.insert(1usize, mutations_1);
                    mutations
                }
                Self::C { bar } => {
                    let mutations_bar = unsafe {
                        ::muon::observe::SerializeObserver::flush(bar)
                    };
                    if mutations_bar.is_replace() {
                        return ::muon::Mutations::replace(unsafe { &*__value });
                    }
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(!mutations_bar.is_empty() as usize);
                    mutations.insert("bar", mutations_bar);
                    mutations
                }
                Self::__Unknown => ::muon::Mutations::new(),
            }
        }
        fn flat_flush(&mut self, __value: *const Foo) -> ::muon::Mutations {
            match self {
                Self::A(u0) => {
                    unsafe { ::muon::observe::SerializeObserver::flat_flush(u0) }
                }
                Self::C { bar } => {
                    let mutations_bar = unsafe {
                        ::muon::observe::SerializeObserver::flush(bar)
                    };
                    let mut mutations = ::muon::Mutations::new()
                        .with_capacity(!mutations_bar.is_empty() as usize)
                        .with_replace(mutations_bar.is_replace());
                    mutations.insert("bar", mutations_bar);
                    mutations
                }
                _ => panic!("flat_flush can only be called on structs and maps"),
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
            self.mutated = true;
            self.variant = FooObserverVariant::__Unknown;
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
        fn invalidate(this: &mut Self) {
            this.mutated = true;
            this.variant = FooObserverVariant::__Unknown;
        }
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
                mutated: false,
                initial: FooObserverInitial::new(__value),
                variant: FooObserverVariant::observe(__value),
                ptr: ::muon::helper::Pointer::new(head),
                phantom: ::std::marker::PhantomData,
            }
        }
        unsafe fn relocate(this: &mut Self, head: &mut S) {
            let __value = head.as_deref_mut();
            unsafe { this.variant.relocate(__value) }
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
        unsafe fn flat_flush(this: &mut Self) -> ::muon::Mutations {
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
    impl ::muon::Observe for Foo {
        type Observer<'ob, S, N> = FooObserver<'ob, S, N>
        where
            Self: 'ob,
            N: ::muon::helper::Unsigned,
            S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
        type Spec = ::muon::observe::DefaultSpec;
    }
    #[automatically_derived]
    impl<'ob, S: ?Sized, N> ::std::fmt::Display for FooObserver<'ob, S, N>
    where
        S: ::muon::helper::AsDeref<N, Target = Foo>,
        N: ::muon::helper::Unsigned,
    {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            ::std::fmt::Display::fmt(self.as_deref(), f)
        }
    }
};
impl Display for Foo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Foo::A(a) => write!(f, "Foo::A({})", a),
            Foo::B(a, b) => write!(f, "Foo::B({}, {})", a, b),
            Foo::C { bar } => write!(f, "Foo::C {{ bar: {} }}", bar),
            Foo::D => write!(f, "Foo::D"),
            Foo::E() => write!(f, "Foo::E()"),
            Foo::F {} => write!(f, "Foo::F {{}}"),
        }
    }
}
