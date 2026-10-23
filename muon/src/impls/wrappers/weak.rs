use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use serde::Serialize;

use crate::Mutations;
use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::{spec_impl_observe_from_ro, spec_impl_ro_observe};
use crate::helper::{AsDeref, AsDerefMut, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{Observer, SerializeObserver};

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
    O: Observer<InnerDepth = Zero>,
{
    unsafe fn observe(head: *mut Self::Head) -> Self {
        unsafe {
            let rc = (&*head).as_deref().upgrade();
            Self {
                mutated: false,
                initial: rc.is_some(),
                inner: rc.map(|ptr| O::observe(std::ptr::from_ref(&*ptr).cast_mut())),
                ptr: Pointer::new_unchecked(head),
                phantom: PhantomData,
            }
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe {
            if let Some(inner) = &mut this.inner
                && let Some(ptr) = (&*head).as_deref().upgrade()
            {
                O::relocate(inner, std::ptr::from_ref(&*ptr).cast_mut());
            }
            Pointer::set_unchecked(&this.ptr, head);
        }
    }
}

impl<O, S: ?Sized, D> SerializeObserver for WeakObserver<O, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target: Weak<O::Head>>,
    O: SerializeObserver<InnerDepth = Zero>,
    O::Head: Serialize + 'static,
    <S::Target as Weak<O::Head>>::Ptr: Serialize,
{
    fn flush(this: &mut Self) -> Mutations<'_> {
        let rc = (*this.ptr).as_deref().upgrade();
        let initial = this.initial;
        this.initial = rc.is_some();
        if !this.mutated {
            if let Some(ob) = &mut this.inner {
                return SerializeObserver::flush(ob);
            } else {
                return Mutations::new();
            }
        }
        this.mutated = false;
        if initial || rc.is_some() {
            Mutations::replace(rc)
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

spec_impl_observe_from_ro!(WeakObserveImpl, std::rc::Weak<Self>, std::rc::Weak<T>, WeakObserver);
spec_impl_ro_observe!(WeakRoObserveImpl, std::rc::Weak<Self>, std::rc::Weak<T>, WeakObserver);

spec_impl_observe_from_ro!(
    SyncWeakObserveImpl,
    std::sync::Weak<Self>,
    std::sync::Weak<T>,
    WeakObserver,
);

spec_impl_ro_observe!(
    SyncWeakRoObserveImpl,
    std::sync::Weak<Self>,
    std::sync::Weak<T>,
    WeakObserver,
);

/// Shared [`SerializeSnapshot::flush`] implementation for [`std::rc::Weak`] and
/// [`std::sync::Weak`].
///
/// The upgraded pointer is a temporary, and the `Weak` does not keep the pointee alive, so any
/// change is emitted as a whole replace that owns the upgraded pointer.
fn flush_weak_snapshot<'a, T, P>(ptr: Option<P>, snapshot: Option<T::Snapshot>) -> Mutations<'a>
where
    T: SerializeSnapshot + ?Sized,
    P: Deref<Target = T> + Serialize + 'a,
{
    let changed = match (&ptr, snapshot) {
        (Some(v), Some(s)) => !(**v).flush(s).is_empty(),
        (None, None) => false,
        _ => true,
    };
    if changed {
        Mutations::replace(ptr)
    } else {
        Mutations::new()
    }
}

impl<T: Snapshot + ?Sized> Snapshot for std::rc::Weak<T> {
    type Snapshot = Option<T::Snapshot>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.upgrade().map(|v| v.to_snapshot())
    }
}

impl<T: SerializeSnapshot + ?Sized> SerializeSnapshot for std::rc::Weak<T> {
    fn flush(&self, snapshot: Self::Snapshot) -> Mutations<'_> {
        flush_weak_snapshot(self.upgrade(), snapshot)
    }
}

impl<T: Snapshot + ?Sized> Snapshot for std::sync::Weak<T> {
    type Snapshot = Option<T::Snapshot>;

    fn to_snapshot(&self) -> Self::Snapshot {
        self.upgrade().map(|v| v.to_snapshot())
    }
}

impl<T: SerializeSnapshot + ?Sized> SerializeSnapshot for std::sync::Weak<T> {
    fn flush(&self, snapshot: Self::Snapshot) -> Mutations<'_> {
        flush_weak_snapshot(self.upgrade(), snapshot)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::sync::Arc;

    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::{Adapter, Json};
    use crate::general::{SerializeSnapshot, Snapshot};
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserver};

    #[test]
    fn snapshot_flush_no_change() {
        let rc = Rc::new("a".to_string());
        let weak = Rc::downgrade(&rc);
        assert!(weak.flush(weak.to_snapshot()).is_empty());
        drop(rc);
        assert!(weak.flush(weak.to_snapshot()).is_empty());
    }

    #[test]
    fn snapshot_flush_outlives_strong_ref() {
        let rc = Rc::new("ab".to_string());
        let weak = Rc::downgrade(&rc);
        let mutations = weak.flush(Some("a".into()));
        // The mutations must keep the pointee alive on their own.
        drop(rc);
        let Json(mutation) = Json::from_mutations(mutations).unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("ab"))));
    }

    #[test]
    fn snapshot_flush_dropped() {
        let arc = Arc::new("a".to_string());
        let weak = Arc::downgrade(&arc);
        let snapshot = weak.to_snapshot();
        drop(arc);
        let Json(mutation) = Json::from_mutations(weak.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(null))));
    }

    #[test]
    fn observer_flush_outlives_strong_ref() {
        let rc1 = Rc::new("a".to_string());
        let rc2 = Rc::new("b".to_string());
        let mut weak = Rc::downgrade(&rc1);
        let mut ob = weak.__observe();
        *ob.tracked_mut() = Rc::downgrade(&rc2);
        let mutations = SerializeObserver::flush(&mut ob);
        // The mutations must keep the pointee alive on their own.
        drop(rc2);
        let Json(mutation) = Json::from_mutations(mutations).unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("b"))));
    }
}
