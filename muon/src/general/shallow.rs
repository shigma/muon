use std::marker::PhantomData;

use crate::helper::Invalidate;
use crate::helper::shallow::{ObserverState, SerializeObserverState, shallow_observer};
use crate::mutation::Mutations;

struct ShallowObserverState<T: ?Sized>(bool, PhantomData<T>);

impl<T: ?Sized> Invalidate<T> for ShallowObserverState<T> {
    fn invalidate(&mut self, _: &T) {
        self.0 = true;
    }
}

impl<T: ?Sized> ObserverState<T> for ShallowObserverState<T> {
    fn observe(_: &T) -> Self {
        Self(false, PhantomData)
    }
}

impl<T: serde::Serialize + ?Sized + 'static> SerializeObserverState<T> for ShallowObserverState<T> {
    fn flush(&mut self, value: &T) -> Mutations {
        if std::mem::take(&mut self.0) {
            Mutations::replace(value)
        } else {
            Mutations::new()
        }
    }
}

shallow_observer! {
    /// A general observer that tracks any mutation access as a change.
    ///
    /// [`ShallowObserver`] uses a simple boolean flag to track whether [`DerefMut`](std::ops::DerefMut)
    /// has been called, treating any mutable access as a change. This makes it extremely efficient with
    /// minimal overhead.
    ///
    /// ## Derive Usage
    ///
    /// Can be used via the `#[muon(shallow)]` attribute in derive macros:
    ///
    /// ```
    /// # use muon::Observe;
    /// # use serde::Serialize;
    /// # #[derive(Serialize)]
    /// # struct ExternalType;
    /// #[derive(Serialize, Observe)]
    /// struct MyStruct {
    ///     #[muon(shallow)]
    ///     external_data: ExternalType,    // ExternalType doesn't implement Observe
    /// }
    /// ```
    ///
    /// ## When to Use
    ///
    /// Despite its limitations, [`ShallowObserver`] is usually the best choice for external types that
    /// don't implement the [`Observe`](crate::Observe) trait, as the performance benefits typically
    /// outweigh the occasional false positive.
    ///
    /// ## Limitations
    ///
    /// 1. **False positives on round-trip changes**: If a value is modified and then restored to its
    ///    original value, it's still reported as changes.
    /// 2. **False positives on non-semantic changes**: Operations that don't affect serialization (such
    ///    as [`Vec::reserve`]) are still reported as changes.
    struct ShallowObserver<T>(T, ShallowObserverState<T>);
}
