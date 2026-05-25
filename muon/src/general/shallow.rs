use std::marker::PhantomData;

use crate::general::{DebugHandler, GeneralHandler, GeneralObserver, ReplaceHandler};
use crate::helper::{AsDeref, Invalidate, Zero};

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
pub type ShallowObserver<'ob, S, D = Zero> = GeneralObserver<'ob, ShallowHandler<<S as AsDeref<D>>::Target>, S, D>;

pub struct ShallowHandler<T: ?Sized> {
    mutated: bool,
    phantom: PhantomData<T>,
}

impl<T: ?Sized> Invalidate<T> for ShallowHandler<T> {
    fn invalidate(&mut self, _: &T) {
        self.mutated = true;
    }
}

impl<T: ?Sized> GeneralHandler for ShallowHandler<T> {
    type Target = T;

    fn observe(_value: &T) -> Self {
        Self {
            mutated: false,
            phantom: PhantomData,
        }
    }
}

impl<T: ?Sized> ReplaceHandler for ShallowHandler<T> {
    fn is_replace(&self, _value: &T) -> bool {
        self.mutated
    }
}

impl<T: ?Sized> DebugHandler for ShallowHandler<T> {
    const NAME: &'static str = "ShallowObserver";
}
