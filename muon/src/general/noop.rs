use std::marker::PhantomData;

use crate::general::{DebugHandler, GeneralHandler, GeneralObserver, ReplaceHandler};
use crate::helper::{AsDeref, Invalidate, Zero};

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
pub type NoopObserver<'ob, S, D = Zero> = GeneralObserver<'ob, NoopHandler<<S as AsDeref<D>>::Target>, S, D>;

pub struct NoopHandler<T: ?Sized>(PhantomData<T>);

impl<T: ?Sized> Invalidate<T> for NoopHandler<T> {
    fn invalidate(&mut self, _: &T) {}
}

impl<T: ?Sized> GeneralHandler for NoopHandler<T> {
    type Target = T;

    fn observe(_value: &T) -> Self {
        Self(PhantomData)
    }
}

impl<T: ?Sized> ReplaceHandler for NoopHandler<T> {
    unsafe fn is_replace(&self, _value: &T) -> bool {
        false
    }
}

impl<T: ?Sized> DebugHandler for NoopHandler<T> {
    const NAME: &'static str = "NoopObserver";
}
