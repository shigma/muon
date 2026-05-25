use ::std::ops::{Deref, DerefMut};
#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;
#[rustfmt::skip]
#[derive(Serialize)]
pub struct Foo<T> {
    #[serde(flatten)]
    a: Vec<T>,
    b: i32,
}
#[rustfmt::skip]
const _: () = {
    pub struct FooObserver<'ob, O> {
        a: O,
        b: ::muon::observe::DefaultObserver<'ob, i32>,
    }
    #[automatically_derived]
    impl<'ob, O> ::std::ops::Deref for FooObserver<'ob, O> {
        type Target = O;
        fn deref(&self) -> &Self::Target {
            &self.a
        }
    }
    #[automatically_derived]
    impl<'ob, O> ::std::ops::DerefMut for FooObserver<'ob, O> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            ::std::ptr::from_mut(self).expose_provenance();
            &mut self.a
        }
    }
    #[automatically_derived]
    impl<'ob, O, N> ::muon::helper::QuasiObserver for FooObserver<'ob, O>
    where
        O: ::muon::helper::QuasiObserver<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDeref<N>,
        N: ::muon::helper::Unsigned,
    {
        type Head = O::Head;
        type OuterDepth = ::muon::helper::Succ<O::OuterDepth>;
        type InnerDepth = N;
        fn invalidate(this: &mut Self) {
            ::muon::helper::QuasiObserver::invalidate(&mut this.b);
            ::muon::helper::QuasiObserver::invalidate(&mut this.a);
        }
    }
    #[automatically_derived]
    impl<'ob, T, O, N> ::muon::observe::Observer for FooObserver<'ob, O>
    where
        Vec<T>: 'ob,
        O: ::muon::observe::Observer<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDerefMut<N, Target = Foo<T>>,
        N: ::muon::helper::Unsigned,
    {
        unsafe fn observe(head: *mut O::Head) -> Self {
            unsafe {
                let __value = ::muon::helper::AsDeref::<N>::as_deref_ptr(head);
                let b = ::muon::observe::Observer::observe(&raw mut (*__value).b);
                let a = ::muon::observe::Observer::observe(head);
                let this = Self { a, b };
                let ptr = O::as_deref_coinductive(&this.a);
                ::muon::helper::Pointer::register_observer(ptr, &this.b);
                this
            }
        }
        unsafe fn relocate(this: &mut Self, head: *mut O::Head) {
            unsafe {
                let __value = ::muon::helper::AsDeref::<N>::as_deref_ptr(head);
                ::muon::observe::Observer::relocate(&mut this.b, &raw mut (*__value).b);
                ::muon::observe::Observer::relocate(&mut this.a, head);
            }
        }
    }
    #[automatically_derived]
    impl<'ob, T, O, N> ::muon::observe::SerializeObserver for FooObserver<'ob, O>
    where
        Foo<T>: ::muon::helper::serde::Serialize + 'static,
        Vec<T>: 'ob,
        O: ::muon::observe::Observer<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDerefMut<N, Target = Foo<T>>,
        N: ::muon::helper::Unsigned,
        O: ::muon::observe::SerializeObserver,
    {
        fn flush(this: &mut Self) -> ::muon::Mutations {
            let mutations_a = ::muon::observe::SerializeObserver::flat_flush(
                &mut this.a,
            );
            let mutations_b = ::muon::observe::SerializeObserver::flush(&mut this.b);
            if mutations_a.is_replace() && mutations_b.is_replace() {
                let head = &**(*this).as_deref_coinductive();
                let value = ::muon::helper::AsDeref::<N>::as_deref(head);
                return ::muon::Mutations::replace(value);
            }
            let mut mutations = ::muon::Mutations::new()
                .with_capacity(mutations_a.len() + !mutations_b.is_empty() as usize);
            mutations.extend(mutations_a);
            mutations.insert("b", mutations_b);
            mutations
        }
        fn flat_flush(this: &mut Self) -> ::muon::Mutations {
            let mutations_a = ::muon::observe::SerializeObserver::flat_flush(
                &mut this.a,
            );
            let mutations_b = ::muon::observe::SerializeObserver::flush(&mut this.b);
            let mut mutations = ::muon::Mutations::new()
                .with_capacity(mutations_a.len() + !mutations_b.is_empty() as usize)
                .with_replace(mutations_a.is_replace() && mutations_b.is_replace());
            mutations.extend(mutations_a);
            mutations.insert("b", mutations_b);
            mutations
        }
    }
    #[automatically_derived]
    impl<T> ::muon::Observe for Foo<T>
    where
        Self: ::muon::helper::serde::Serialize,
        Vec<T>: ::muon::Observe,
    {
        type Observer<'ob, S, N> = FooObserver<
            'ob,
            ::muon::observe::DefaultObserver<'ob, Vec<T>, S, ::muon::helper::Succ<N>>,
        >
        where
            Self: 'ob,
            N: ::muon::helper::Unsigned,
            S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
        type Spec = ::muon::observe::DefaultSpec;
    }
    #[automatically_derived]
    unsafe impl<T> ::muon::helper::DerefPtr for Foo<T> {
        unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
            unsafe { &raw mut (*this).a }
        }
    }
};
impl<T> Deref for Foo<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.a
    }
}
impl<T> DerefMut for Foo<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.a
    }
}
#[rustfmt::skip]
#[derive(Serialize)]
pub struct Bar(Qux, i32);
#[rustfmt::skip]
const _: () = {
    pub struct BarObserver<'ob, O>(O, ::muon::observe::DefaultObserver<'ob, i32>);
    #[automatically_derived]
    impl<'ob, O> ::std::ops::Deref for BarObserver<'ob, O> {
        type Target = O;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    #[automatically_derived]
    impl<'ob, O> ::std::ops::DerefMut for BarObserver<'ob, O> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            ::std::ptr::from_mut(self).expose_provenance();
            &mut self.0
        }
    }
    #[automatically_derived]
    impl<'ob, O, N> ::muon::helper::QuasiObserver for BarObserver<'ob, O>
    where
        O: ::muon::helper::QuasiObserver<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDeref<N>,
        N: ::muon::helper::Unsigned,
    {
        type Head = O::Head;
        type OuterDepth = ::muon::helper::Succ<O::OuterDepth>;
        type InnerDepth = N;
        fn invalidate(this: &mut Self) {
            ::muon::helper::QuasiObserver::invalidate(&mut this.1);
            ::muon::helper::QuasiObserver::invalidate(&mut this.0);
        }
    }
    #[automatically_derived]
    impl<'ob, O, N> ::muon::observe::Observer for BarObserver<'ob, O>
    where
        O: ::muon::observe::Observer<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDerefMut<N, Target = Bar>,
        N: ::muon::helper::Unsigned,
    {
        unsafe fn observe(head: *mut O::Head) -> Self {
            unsafe {
                let __value = ::muon::helper::AsDeref::<N>::as_deref_ptr(head);
                let observer_1 = ::muon::observe::Observer::observe(
                    &raw mut (*__value).1,
                );
                let observer_0 = ::muon::observe::Observer::observe(head);
                let this = Self(observer_0, observer_1);
                let ptr = O::as_deref_coinductive(&this.0);
                ::muon::helper::Pointer::register_observer(ptr, &this.1);
                this
            }
        }
        unsafe fn relocate(this: &mut Self, head: *mut O::Head) {
            unsafe {
                let __value = ::muon::helper::AsDeref::<N>::as_deref_ptr(head);
                ::muon::observe::Observer::relocate(&mut this.1, &raw mut (*__value).1);
                ::muon::observe::Observer::relocate(&mut this.0, head);
            }
        }
    }
    #[automatically_derived]
    impl<'ob, O, N> ::muon::observe::SerializeObserver for BarObserver<'ob, O>
    where
        O: ::muon::observe::Observer<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDerefMut<N, Target = Bar>,
        N: ::muon::helper::Unsigned,
        O: ::muon::observe::SerializeObserver,
    {
        fn flush(this: &mut Self) -> ::muon::Mutations {
            let mutations_0 = ::muon::observe::SerializeObserver::flush(&mut this.0);
            let mutations_1 = ::muon::observe::SerializeObserver::flush(&mut this.1);
            if mutations_0.is_replace() && mutations_1.is_replace() {
                let head = &**(*this).as_deref_coinductive();
                let value = ::muon::helper::AsDeref::<N>::as_deref(head);
                return ::muon::Mutations::replace(value);
            }
            let mut mutations = ::muon::Mutations::new()
                .with_capacity(
                    !mutations_0.is_empty() as usize + !mutations_1.is_empty() as usize,
                );
            mutations.insert(0usize, mutations_0);
            mutations.insert(1usize, mutations_1);
            mutations
        }
        fn flat_flush(this: &mut Self) -> ::muon::Mutations {
            let mutations_0 = ::muon::observe::SerializeObserver::flush(&mut this.0);
            let mutations_1 = ::muon::observe::SerializeObserver::flush(&mut this.1);
            let mut mutations = ::muon::Mutations::new()
                .with_capacity(
                    !mutations_0.is_empty() as usize + !mutations_1.is_empty() as usize,
                )
                .with_replace(mutations_0.is_replace() && mutations_1.is_replace());
            mutations.insert(0usize, mutations_0);
            mutations.insert(1usize, mutations_1);
            mutations
        }
    }
    #[automatically_derived]
    impl ::muon::Observe for Bar
    where
        Qux: ::muon::Observe,
    {
        type Observer<'ob, S, N> = BarObserver<
            'ob,
            ::muon::general::ShallowObserver<'ob, S, ::muon::helper::Succ<N>>,
        >
        where
            Self: 'ob,
            N: ::muon::helper::Unsigned,
            S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
        type Spec = ::muon::observe::DefaultSpec;
    }
    #[automatically_derived]
    unsafe impl ::muon::helper::DerefPtr for Bar {
        unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
            unsafe { &raw mut (*this).0 }
        }
    }
};
impl Deref for Bar {
    type Target = Qux;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Bar {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
#[rustfmt::skip]
#[derive(Serialize)]
pub struct Qux(pub i32);
#[rustfmt::skip]
const _: () = {
    pub struct QuxObserver<O>(pub O);
    #[automatically_derived]
    impl<O> ::std::ops::Deref for QuxObserver<O> {
        type Target = O;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    #[automatically_derived]
    impl<O> ::std::ops::DerefMut for QuxObserver<O> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            ::std::ptr::from_mut(self).expose_provenance();
            &mut self.0
        }
    }
    #[automatically_derived]
    impl<O, N> ::muon::helper::QuasiObserver for QuxObserver<O>
    where
        O: ::muon::helper::QuasiObserver<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDeref<N>,
        N: ::muon::helper::Unsigned,
    {
        type Head = O::Head;
        type OuterDepth = ::muon::helper::Succ<O::OuterDepth>;
        type InnerDepth = N;
        fn invalidate(this: &mut Self) {
            ::muon::helper::QuasiObserver::invalidate(&mut this.0);
        }
    }
    #[automatically_derived]
    impl<O, N> ::muon::observe::Observer for QuxObserver<O>
    where
        O: ::muon::observe::Observer<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDerefMut<N, Target = Qux>,
        N: ::muon::helper::Unsigned,
    {
        unsafe fn observe(head: *mut O::Head) -> Self {
            unsafe {
                let observer_0 = ::muon::observe::Observer::observe(head);
                Self(observer_0)
            }
        }
        unsafe fn relocate(this: &mut Self, head: *mut O::Head) {
            unsafe {
                ::muon::observe::Observer::relocate(&mut this.0, head);
            }
        }
    }
    #[automatically_derived]
    impl<O, N> ::muon::observe::SerializeObserver for QuxObserver<O>
    where
        O: ::muon::observe::Observer<InnerDepth = ::muon::helper::Succ<N>>,
        O::Head: ::muon::helper::AsDerefMut<N, Target = Qux>,
        N: ::muon::helper::Unsigned,
        O: ::muon::observe::SerializeObserver,
    {
        fn flush(this: &mut Self) -> ::muon::Mutations {
            ::muon::observe::SerializeObserver::flush(&mut this.0)
        }
        fn flat_flush(this: &mut Self) -> ::muon::Mutations {
            ::muon::observe::SerializeObserver::flat_flush(&mut this.0)
        }
    }
    #[automatically_derived]
    impl ::muon::Observe for Qux
    where
        i32: ::muon::Observe,
    {
        type Observer<'ob, S, N> = QuxObserver<
            ::muon::observe::DefaultObserver<'ob, i32, S, ::muon::helper::Succ<N>>,
        >
        where
            Self: 'ob,
            N: ::muon::helper::Unsigned,
            S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
        type Spec = ::muon::observe::DefaultSpec;
    }
    #[automatically_derived]
    unsafe impl ::muon::helper::DerefPtr for Qux {
        unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
            unsafe { &raw mut (*this).0 }
        }
    }
};
impl Deref for Qux {
    type Target = i32;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Qux {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
