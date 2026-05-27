use std::marker::PhantomData;

use crate::helper::Invalidate;
use crate::helper::shallow::{ObserverState, SerializeObserverState, shallow_observer};
use crate::mutation::Mutations;

struct NoopObserverState<T: ?Sized>(PhantomData<T>);

impl<T: ?Sized> Invalidate<T> for NoopObserverState<T> {
    fn invalidate(&mut self, _: &T) {}
}

impl<T: ?Sized> ObserverState<T> for NoopObserverState<T> {
    fn observe(_: &T) -> Self {
        Self(PhantomData)
    }
}

impl<T: ?Sized> SerializeObserverState<T> for NoopObserverState<T> {
    fn flush(&mut self, _: &T) -> Mutations {
        Mutations::new()
    }
}

shallow_observer! {
    /// A general observer that never reports changes.
    ///
    /// [`NoopObserver`] is a no-operation [`Observer`](crate::observe::Observer) that always returns
    /// [`None`] when collecting changes, effectively ignoring all mutations to the observed value.
    ///
    /// ## Derive Usage
    ///
    /// Can be used via the `#[muon(noop)]` attribute in derive macros:
    ///
    /// ```
    /// # use muon::Observe;
    /// # use serde::Serialize;
    /// #[derive(Serialize, Observe)]
    /// struct MyStruct {
    ///     important_field: String,
    ///     #[muon(noop)]
    ///     cache: String,      // Changes to cache are not tracked
    /// }
    /// ```
    ///
    /// ## When to Use
    ///
    /// Use [`NoopObserver`] for fields that:
    /// - Are only used internally and not part of the public state
    /// - Should not trigger change notifications.
    struct NoopObserver<T>(T, NoopObserverState<T>);
}
