#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;
#[rustfmt::skip]
#[derive(Serialize)]
#[serde(bound = "S: Serialize, U: Serialize, V: Serialize")]
pub struct Foo<'a, S, T, U, V, const N: usize> {
    #[serde(serialize_with = "serialize_mut_array")]
    a: &'a mut [S; N],
    #[serde(skip)]
    pub b: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c: Option<U>,
    pub d: V,
}
#[rustfmt::skip]
const _: () = {
    pub struct FooObserver<
        'ob,
        'a,
        S,
        T,
        U,
        V,
        const N: usize,
        _S: ?Sized,
        _N = ::muon::helper::Zero,
    >
    where
        &'a mut [S; N]: ::muon::Observe + 'ob,
        Option<U>: ::muon::Observe + 'ob,
    {
        a: ::muon::observe::DefaultObserver<'ob, &'a mut [S; N]>,
        pub b: ::muon::helper::Pointer<Option<T>>,
        pub c: ::muon::observe::DefaultObserver<'ob, Option<U>>,
        pub d: ::muon::general::ShallowObserver<'ob, V, V>,
        __ptr: ::muon::helper::Pointer<_S>,
        __phantom: ::std::marker::PhantomData<&'ob mut _N>,
    }
    #[automatically_derived]
    impl<'ob, 'a, S, T, U, V, const N: usize, _S: ?Sized, _N> ::std::ops::Deref
    for FooObserver<'ob, 'a, S, T, U, V, N, _S, _N>
    where
        &'a mut [S; N]: ::muon::Observe,
        Option<U>: ::muon::Observe,
    {
        type Target = ::muon::helper::Pointer<_S>;
        fn deref(&self) -> &Self::Target {
            &self.__ptr
        }
    }
    #[automatically_derived]
    impl<'ob, 'a, S, T, U, V, const N: usize, _S: ?Sized, _N> ::std::ops::DerefMut
    for FooObserver<'ob, 'a, S, T, U, V, N, _S, _N>
    where
        &'a mut [S; N]: ::muon::Observe,
        Option<U>: ::muon::Observe,
    {
        fn deref_mut(&mut self) -> &mut Self::Target {
            ::std::ptr::from_mut(self).expose_provenance();
            ::muon::helper::QuasiObserver::invalidate(&mut self.__ptr);
            ::muon::helper::QuasiObserver::invalidate(&mut self.a);
            ::muon::helper::QuasiObserver::invalidate(&mut self.c);
            ::muon::helper::QuasiObserver::invalidate(&mut self.d);
            &mut self.__ptr
        }
    }
    #[automatically_derived]
    impl<
        'ob,
        'a,
        S,
        T,
        U,
        V,
        const N: usize,
        _S: ?Sized,
        _N,
    > ::muon::helper::QuasiObserver for FooObserver<'ob, 'a, S, T, U, V, N, _S, _N>
    where
        _S: ::muon::helper::AsDeref<_N>,
        &'a mut [S; N]: ::muon::Observe,
        Option<U>: ::muon::Observe,
        _N: ::muon::helper::Unsigned,
    {
        type Head = _S;
        type OuterDepth = ::muon::helper::Succ<::muon::helper::Zero>;
        type InnerDepth = _N;
        fn invalidate(this: &mut Self) {
            ::muon::helper::QuasiObserver::invalidate(&mut this.a);
            ::muon::helper::QuasiObserver::invalidate(&mut this.c);
            ::muon::helper::QuasiObserver::invalidate(&mut this.d);
        }
    }
    #[automatically_derived]
    impl<'ob, 'a, S, T, U, V, const N: usize, _S: ?Sized, _N> ::muon::observe::Observer
    for FooObserver<'ob, 'a, S, T, U, V, N, _S, _N>
    where
        Option<T>: 'ob,
        V: 'ob,
        &'a mut [S; N]: ::muon::Observe,
        Option<U>: ::muon::Observe,
        _S: ::muon::helper::AsDerefMut<_N, Target = Foo<'a, S, T, U, V, N>>,
        _N: ::muon::helper::Unsigned,
    {
        unsafe fn observe(head: *mut _S) -> Self {
            unsafe {
                let __value = ::muon::helper::AsDeref::<_N>::as_deref_ptr(head);
                let a = ::muon::observe::Observer::observe(&raw mut (*__value).a);
                let b = ::muon::helper::Pointer::new_unchecked(&raw mut (*__value).b);
                let c = ::muon::observe::Observer::observe(&raw mut (*__value).c);
                let d = ::muon::observe::Observer::observe(&raw mut (*__value).d);
                Self {
                    a,
                    b,
                    c,
                    d,
                    __ptr: ::muon::helper::Pointer::new_unchecked(head),
                    __phantom: ::std::marker::PhantomData,
                }
            }
        }
        unsafe fn relocate(this: &mut Self, head: *mut _S) {
            unsafe {
                let __value = ::muon::helper::AsDeref::<_N>::as_deref_ptr(head);
                ::muon::observe::Observer::relocate(&mut this.a, &raw mut (*__value).a);
                ::muon::helper::Pointer::set_unchecked(&this.b, &raw mut (*__value).b);
                ::muon::observe::Observer::relocate(&mut this.c, &raw mut (*__value).c);
                ::muon::observe::Observer::relocate(&mut this.d, &raw mut (*__value).d);
                ::muon::helper::Pointer::set_unchecked(this, head);
            }
        }
    }
    #[automatically_derived]
    impl<
        'ob,
        'a,
        S,
        T,
        U,
        V,
        const N: usize,
        _S: ?Sized,
        _N,
    > ::muon::observe::SerializeObserver for FooObserver<'ob, 'a, S, T, U, V, N, _S, _N>
    where
        Foo<'a, S, T, U, V, N>: ::muon::helper::serde::Serialize + 'static,
        Option<T>: 'ob,
        V: 'ob,
        &'a mut [S; N]: ::muon::Observe,
        Option<U>: ::muon::Observe,
        _S: ::muon::helper::AsDerefMut<_N, Target = Foo<'a, S, T, U, V, N>>,
        _N: ::muon::helper::Unsigned,
        ::muon::observe::DefaultObserver<
            'ob,
            &'a mut [S; N],
        >: ::muon::observe::SerializeObserver,
        ::muon::observe::DefaultObserver<
            'ob,
            Option<U>,
        >: ::muon::observe::SerializeObserver,
        ::muon::general::ShallowObserver<'ob, V, V>: ::muon::observe::SerializeObserver,
    {
        fn flush(this: &mut Self) -> ::muon::Mutations {
            let mutations_a = ::muon::observe::SerializeObserver::flush(&mut this.a);
            let mutations_c = ::muon::observe::SerializeObserver::flush(&mut this.c);
            let mutations_d = ::muon::observe::SerializeObserver::flush(&mut this.d);
            if mutations_a.is_replace() && mutations_c.is_replace()
                && mutations_d.is_replace()
            {
                let value = ::muon::helper::QuasiObserver::untracked_ref(&*this);
                return ::muon::Mutations::replace(value);
            }
            let mut mutations = ::muon::Mutations::new()
                .with_capacity(
                    !mutations_a.is_empty() as usize + !mutations_c.is_empty() as usize
                        + !mutations_d.is_empty() as usize,
                );
            let __inner = ::muon::helper::QuasiObserver::untracked_ref(&*this);
            mutations.insert("a", mutations_a);
            if !mutations_c.is_empty() && Option::is_none(&__inner.c) {
                mutations.insert("c", ::muon::Mutations::delete());
            } else {
                mutations.insert("c", mutations_c);
            }
            mutations.insert("d", mutations_d);
            mutations
        }
        fn flat_flush(this: &mut Self) -> ::muon::Mutations {
            let mutations_a = ::muon::observe::SerializeObserver::flush(&mut this.a);
            let mutations_c = ::muon::observe::SerializeObserver::flush(&mut this.c);
            let mutations_d = ::muon::observe::SerializeObserver::flush(&mut this.d);
            let mut mutations = ::muon::Mutations::new()
                .with_capacity(
                    !mutations_a.is_empty() as usize + !mutations_c.is_empty() as usize
                        + !mutations_d.is_empty() as usize,
                )
                .with_replace(
                    mutations_a.is_replace() && mutations_c.is_replace()
                        && mutations_d.is_replace(),
                );
            let __inner = ::muon::helper::QuasiObserver::untracked_ref(&*this);
            mutations.insert("a", mutations_a);
            if !mutations_c.is_empty() && Option::is_none(&__inner.c) {
                mutations.insert("c", ::muon::Mutations::delete());
            } else {
                mutations.insert("c", mutations_c);
            }
            mutations.insert("d", mutations_d);
            mutations
        }
    }
    #[automatically_derived]
    impl<'a, S, T, U, V, const N: usize> ::muon::Observe for Foo<'a, S, T, U, V, N>
    where
        Self: ::muon::helper::serde::Serialize,
        &'a mut [S; N]: ::muon::Observe,
        Option<U>: ::muon::Observe,
    {
        type Observer<'ob, _S, _N> = FooObserver<'ob, 'a, S, T, U, V, N, _S, _N>
        where
            Self: 'ob,
            &'a mut [S; N]: 'ob,
            Option<U>: 'ob,
            _N: ::muon::helper::Unsigned,
            _S: ::muon::helper::AsDerefMut<_N, Target = Self> + ?Sized + 'ob;
        type Spec = ::muon::observe::DefaultSpec;
    }
};
#[rustfmt::skip]
fn serialize_mut_array<T, S, const N: usize>(
    a: &&mut [T; N],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: serde::Serializer,
{
    <[_]>::serialize(&**a, serializer)
}
