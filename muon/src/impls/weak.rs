use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use serde::Serialize;

use crate::Mutations;
use crate::general::Snapshot;
use crate::helper::macros::{spec_impl_observe_from_ref, spec_impl_ref_observe};
use crate::helper::{AsDeref, AsDerefMut, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::mutation::SerializeRef;
use crate::observe::{Observer, RefObserver, SerializeObserver};

trait Weak<T: ?Sized> {
    type Ptr: Deref<Target = T>;

    fn upgrade(&self) -> Option<Self::Ptr>;
}

impl<T: ?Sized> Weak<T> for std::rc::Weak<T> {
    type Ptr = std::rc::Rc<T>;

    fn upgrade(&self) -> Option<Self::Ptr> {
        self.upgrade()
    }
}

impl<T: ?Sized> Weak<T> for std::sync::Weak<T> {
    type Ptr = std::sync::Arc<T>;

    fn upgrade(&self) -> Option<Self::Ptr> {
        self.upgrade()
    }
}

/// Observer implementation for [`std::rc::Weak<T>`] and [`std::sync::Weak<T>`].
pub struct WeakObserver<O, S: ?Sized, D> {
    ptr: Pointer<S>,
    mutated: bool,
    initial: bool,
    inner: Option<O>,
    phantom: PhantomData<D>,
}

impl<O, S: ?Sized, D> Deref for WeakObserver<O, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<O, S: ?Sized, D> DerefMut for WeakObserver<O, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mutated = true;
        self.inner = None;
        &mut self.ptr
    }
}

impl<O, S: ?Sized, D> QuasiObserver for WeakObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D>,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        this.mutated = true;
        this.inner = None;
    }
}

impl<O, S: ?Sized, D> Observer for WeakObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target: Weak<O::Head>>,
    O: RefObserver<InnerDepth = Zero>,
{
    fn observe(head: &mut Self::Head) -> Self {
        let rc = (*head).as_deref().upgrade();
        Self {
            mutated: false,
            initial: rc.is_some(),
            inner: rc.map(|ptr| O::observe(&*ptr)),
            ptr: Pointer::new(head),
            phantom: PhantomData,
        }
    }

    unsafe fn relocate(this: &mut Self, head: &mut Self::Head) {
        if let Some(inner) = &mut this.inner
            && let Some(ptr) = (*head).as_deref().upgrade()
        {
            unsafe { O::relocate(inner, &*ptr) }
        }
        Pointer::set(&this.ptr, head);
    }
}

impl<O, S: ?Sized, D> RefObserver for WeakObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target: Weak<O::Head>>,
    O: RefObserver<InnerDepth = Zero>,
{
    fn observe(head: &Self::Head) -> Self {
        let rc = head.as_deref().upgrade();
        Self {
            ptr: Pointer::new(head),
            mutated: false,
            initial: rc.is_some(),
            inner: rc.map(|ptr| O::observe(&*ptr)),
            phantom: PhantomData,
        }
    }

    unsafe fn relocate(this: &mut Self, head: &Self::Head) {
        if let Some(inner) = &mut this.inner
            && let Some(ptr) = head.as_deref().upgrade()
        {
            unsafe { O::relocate(inner, &*ptr) }
        }
    }
}

impl<O, S: ?Sized, D> SerializeObserver for WeakObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target: Weak<O::Head>>,
    O: SerializeObserver<InnerDepth = Zero>,
    O::Head: Serialize + 'static,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        let rc = (*this.ptr).as_deref().upgrade();
        let initial = this.initial;
        this.initial = rc.is_some();
        if !this.mutated {
            if let Some(ob) = &mut this.inner {
                return unsafe { SerializeObserver::flush(ob) };
            } else {
                return Mutations::new();
            }
        }
        this.mutated = false;
        if initial || rc.is_some() {
            Mutations::replace_owned(rc.as_deref().map(|v| SerializeRef(v)))
        } else {
            Mutations::new()
        }
    }
}

impl<O, S: ?Sized, D> Debug for WeakObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D>,
    S::Target: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WeakObserver").field(&self.untracked_ref()).finish()
    }
}

spec_impl_observe_from_ref!(WeakObserveImpl, std::rc::Weak<Self>, std::rc::Weak<T>, WeakObserver);
spec_impl_ref_observe!(WeakRefObserveImpl, std::rc::Weak<Self>, std::rc::Weak<T>, WeakObserver);

impl<T: Snapshot + ?Sized> Snapshot for std::rc::Weak<T> {
    type Snapshot = Option<T::Snapshot>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.upgrade().map(|v| v.to_snapshot())
    }

    fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
        self.upgrade()
            .zip(snapshot.as_ref())
            .is_some_and(|(v, snapshot)| v.eq_snapshot(snapshot))
    }
}

impl<T: Snapshot + ?Sized> Snapshot for std::sync::Weak<T> {
    type Snapshot = Option<T::Snapshot>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.upgrade().map(|v| v.to_snapshot())
    }

    fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
        self.upgrade()
            .zip(snapshot.as_ref())
            .is_some_and(|(v, snapshot)| v.eq_snapshot(snapshot))
    }
}
