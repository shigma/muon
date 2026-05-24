use std::ptr::NonNull;

use crate::Observe;
use crate::general::{DebugHandler, GeneralHandler, GeneralObserver, ReplaceHandler};
use crate::helper::macros::default_impl_ref_observe;
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Unsigned};
use crate::observe::DefaultSpec;

/// A general observer implementation for reference types.
///
/// This observer stores the initial pointer value and compares it with the current value at
/// collection time using [`std::ptr::eq`]. A change is detected if the reference now points to a
/// different memory location.
///
/// ## Limitations
///
/// - **False negatives**: If the referenced value contains interior mutability and is mutated
///   without changing the pointer, the mutation will not be detected.
/// - **False positives**: If two distinct references point to equal values, changing from one to
///   the other will be detected as a change, even if the underlying value is effectively the same.
///
/// ## When to Use
///
/// Use [`PointerObserver`] for types where:
/// 1. Pointer identity is a reliable indicator of value identity
/// 2. Value comparison is expensive or unavailable
/// 3. The type has no interior mutability
///
/// For types where value comparison is cheap and preferred, consider using
/// [`SnapshotObserver`](crate::general::SnapshotObserver) for references.
pub type PointerObserver<'ob, S, D> = GeneralObserver<'ob, PointerHandler<<S as AsDeref<D>>::Target>, S, D>;

pub struct PointerHandler<T: ?Sized> {
    ptr: Option<NonNull<T>>,
}

impl<T: ?Sized> Invalidate<T> for PointerHandler<T> {
    fn invalidate(&mut self, _: &T) {}
}

impl<T: ?Sized> GeneralHandler for PointerHandler<T> {
    type Target = T;

    fn observe(value: &T) -> Self {
        Self {
            ptr: Some(NonNull::from(value)),
        }
    }
}

impl<T: ?Sized> ReplaceHandler for PointerHandler<T> {
    unsafe fn is_replace(&self, value: &T) -> bool {
        !std::ptr::eq(
            value,
            self.ptr
                .expect("pointer should not be null in GeneralHandler::flush")
                .as_ptr(),
        )
    }
}

impl<T: ?Sized> DebugHandler for PointerHandler<T> {
    const NAME: &'static str = "PointerObserver";
}

impl Observe for std::fmt::Arguments<'_> {
    type Observer<'ob, S, D>
        = PointerObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ref_observe! {
    impl RefObserve for std::fmt::Arguments<'_>;
}
