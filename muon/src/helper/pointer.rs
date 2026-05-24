use std::cell::{Cell, UnsafeCell};
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

use crate::helper::quasi::Invalidate;
use crate::helper::{AsDeref, QuasiObserver, Unsigned};

/// Recovers a `*const S` with [exposed provenance](std::ptr#exposed-provenance) from a raw pointer
/// that may carry a stale tag.
///
/// For thin pointers (`S: Sized`), this replaces the entire pointer. For fat pointers (`S:
/// ?Sized`), it replaces only the data-address word while preserving the metadata.
///
/// At the bit level this is a no-op (the address is unchanged). At the abstract-machine level it
/// replaces stale provenance: Miri's byte-level provenance model tracks the [`std::ptr::write`] and
/// records the new provenance on the overwritten bytes.
///
/// Once [`ptr_metadata`](https://github.com/rust-lang/rust/issues/81513) is stabilized this can
/// be replaced with [`std::ptr::from_raw_parts`].
fn recover_provenance<S: ?Sized>(raw: *const S) -> *const S {
    let exposed = std::ptr::with_exposed_provenance::<u8>(raw.cast::<u8>().addr());
    let mut result = raw;
    // SAFETY: `*const S` always starts with a data-address word (both thin and fat pointers).
    // Writing a `*const u8` into that word replaces only the data address, preserving metadata.
    unsafe { std::ptr::write((&raw mut result).cast::<*const u8>(), exposed) }
    result
}

/// Recovers a `*mut S` with [exposed provenance](std::ptr#exposed-provenance) from a raw pointer
/// that may carry a stale tag.
///
/// For thin pointers (`S: Sized`), this replaces the entire pointer. For fat pointers (`S:
/// ?Sized`), it replaces only the data-address word while preserving the metadata.
///
/// At the bit level this is a no-op (the address is unchanged). At the abstract-machine level it
/// replaces stale provenance: Miri's byte-level provenance model tracks the [`std::ptr::write`] and
/// records the new provenance on the overwritten bytes.
///
/// Once [`ptr_metadata`](https://github.com/rust-lang/rust/issues/81513) is stabilized this can
/// be replaced with [`std::ptr::from_raw_parts_mut`].
fn recover_provenance_mut<S: ?Sized>(raw: *mut S) -> *mut S {
    let exposed = std::ptr::with_exposed_provenance_mut::<u8>(raw.cast::<u8>().addr());
    let mut result = raw;
    unsafe { std::ptr::write((&raw mut result).cast::<*mut u8>(), exposed) }
    result
}

/// An internal pointer type for observer dereference chains.
///
/// [`Pointer`] is a specialized pointer type used exclusively within observer implementations to
/// store references to observed values. It serves as a critical component in the
/// observer dereference chain, allowing multiple levels of observers to coexist while maintaining
/// access to the original value.
///
/// ## Purpose
///
/// When observing types that already implement [`Deref`] (like [`Vec<T>`]), we need a way to break
/// the dereference chain to insert observer logic at multiple levels. [`Pointer`] provides this
/// break point, enabling chains like: [`VecObserver`](crate::impls::VecObserver) →
/// [`SliceObserver`](crate::impls::SliceObserver) → [`Pointer<Vec<T>>`](Pointer) → [`Vec<T>`] →
/// [`[T]`](std::slice).
///
/// ## Safety
///
/// This type uses raw pointers internally and relies on several safety invariants:
///
/// 1. **Lifetime tracking**: The lifetime `'ob` in observers ensures the pointed-to value remains
///    valid for the observer's lifetime.
/// 2. **Single ownership**: Each [`Pointer`] assumes exclusive access to its referenced value
///    during the observer's lifetime.
///
/// ## Interior Mutability
///
/// The `inner` field uses [`Cell`] for interior mutability, allowing [`Pointer::set`] to update
/// the address through a shared reference. This is needed when container reallocation moves
/// elements — [`Observer::relocate`](crate::observe::Observer::relocate) calls [`Pointer::set`]
/// through `&self` rather than `&mut self`.
///
/// ## Fallback Invalidation
///
/// The `states` field stores `(offset, invalidate_fn)` pairs for the fallback invalidation
/// mechanism. When [`DerefMut`] is triggered on a tail observer, it iterates all registered
/// entries and calls their invalidation functions. Each entry records the byte offset from this
/// [`Pointer`] to a sibling observer or state, plus a type-erased function that calls
/// [`Invalidate::invalidate`] or [`QuasiObserver::invalidate`] on it.
///
/// See the [Inline-Field Invariant](crate::observe::Observer#inline-field-invariant) section on
/// [`Observer`](crate::observe::Observer) for the provenance requirements and registration rules
/// that govern this mechanism.
///
/// ## Internal Use Only
///
/// This type is not intended for direct use outside of observer implementations. All safety
/// invariants are maintained by the observer infrastructure when used correctly within that
/// context.
pub struct Pointer<S: ?Sized> {
    inner: Cell<NonNull<S>>,
    #[expect(clippy::type_complexity)]
    pub(crate) states: UnsafeCell<Vec<(isize, unsafe fn(*mut u8, &S))>>,
}

impl<S: ?Sized> Pointer<S> {
    /// Creates a new pointer from a reference.
    ///
    /// The returned pointer will remain valid as long as the original reference remains valid,
    /// which is enforced by the lifetime parameter in observer types.
    pub fn new(head: impl Into<NonNull<S>>) -> Self {
        let ptr = head.into();
        ptr.cast::<u8>().expose_provenance();
        Pointer {
            inner: Cell::new(ptr),
            states: UnsafeCell::new(Vec::new()),
        }
    }

    /// Retrieves the internal raw pointer.
    pub const fn get(this: &Self) -> NonNull<S> {
        this.inner.get()
    }

    /// Updates the internal pointer from a reference.
    ///
    /// This method is primarily used when observed collections (like [`Vec`]) reallocate their
    /// internal storage. When a vector grows and moves its elements to a new memory location,
    /// any existing [`Pointer`] instances pointing to those elements become invalid. This method
    /// allows updating those pointers to point to the elements' new locations.
    pub fn set(this: &Self, head: impl Into<NonNull<S>>) {
        let ptr = head.into();
        ptr.cast::<u8>().expose_provenance();
        this.inner.set(ptr);
    }

    /// Returns a reference to the pointed value.
    ///
    /// Uses [exposed provenance](std::ptr#exposed-provenance) to recover a valid pointer tag.
    /// The original provenance is deposited into the global pool during [`Pointer::new`] or
    /// [`Pointer::set`]. This ensures that even if intermediate tags on the borrow stack have
    /// been invalidated (e.g., by a parent observer's Unique retag during
    /// [`tracked_mut`](QuasiObserver::tracked_mut)), the recovered reference derives
    /// from a still-live provenance.
    ///
    /// ## Safety
    ///
    /// The caller must ensure that:
    /// 1. The original value this pointer was created from is still valid.
    /// 2. No mutable references to the same value exist elsewhere.
    ///
    /// These invariants are automatically maintained when using [`Pointer`] within the observer
    /// infrastructure, but must be manually verified if called directly.
    pub unsafe fn as_ref<'ob>(this: &Self) -> &'ob S {
        unsafe { &*recover_provenance(this.inner.get().as_ptr()) }
    }

    /// Returns a mutable reference to the pointed value.
    ///
    /// Uses [exposed provenance](std::ptr#exposed-provenance) to recover a valid pointer tag.
    /// The original provenance is deposited into the global pool during [`Pointer::new`] or
    /// [`Pointer::set`]. This ensures that even if intermediate tags on the borrow stack have
    /// been invalidated (e.g., by a parent observer's Unique retag during
    /// [`tracked_mut`](QuasiObserver::tracked_mut)), the recovered reference derives
    /// from a still-live provenance.
    ///
    /// ## Safety
    ///
    /// The caller must ensure that:
    /// 1. The original value this pointer was created from is still valid.
    /// 2. No other references (mutable or immutable) to the same value exist elsewhere.
    /// 3. The returned reference is used in a way that maintains Rust's aliasing rules.
    ///
    /// These invariants are automatically maintained when using [`Pointer`] within the observer
    /// infrastructure, but must be manually verified if called directly.
    pub unsafe fn as_mut<'ob>(this: &Self) -> &'ob mut S {
        unsafe { &mut *recover_provenance_mut(this.inner.get().as_ptr()) }
    }

    /// Registers an [`Invalidate`] implementor for fallback invalidation.
    ///
    /// When [`DerefMut`] is triggered on a tail observer, it calls the invalidation function on the
    /// [`Pointer`], which iterates all registered entries and calls their invalidation functions.
    /// For states registered via this method, the erased function calls
    /// [`Invalidate::invalidate`] with a reference to the observed value.
    ///
    /// The `state` must be an inline field relative to this [`Pointer`] (fixed byte offset,
    /// invariant under moves). This is guaranteed by the inline-field invariant.
    pub fn register_state<O, D>(this: &Self, state: &O)
    where
        D: Unsigned,
        S: AsDeref<D>,
        O: Invalidate<S::Target>,
    {
        unsafe fn invalidate<O, D, S>(ptr: *mut u8, value: &S)
        where
            D: Unsigned,
            S: AsDeref<D> + ?Sized,
            O: Invalidate<S::Target>,
        {
            let state = unsafe { &mut *(ptr as *mut O) };
            O::invalidate(state, value.as_deref());
        }

        let offset = state as *const _ as isize - this as *const _ as isize;
        let invalidate: unsafe fn(*mut u8, &S) = invalidate::<O, D, S>;
        unsafe { &mut *this.states.get() }.push((offset, invalidate));
    }

    /// Registers a [`QuasiObserver`] implementor for fallback invalidation.
    ///
    /// Similar to [`register_state`](Self::register_state), but for observer types. The erased
    /// function calls [`QuasiObserver::invalidate`] (which does not need the observed value).
    ///
    /// The `observer` must be an inline field relative to this [`Pointer`] (fixed byte offset,
    /// invariant under moves). This is guaranteed by the inline-field invariant.
    pub fn register_observer<O: QuasiObserver>(this: &Self, observer: &O) {
        unsafe fn invalidate<O: QuasiObserver, S: ?Sized>(ptr: *mut u8, _: &S) {
            let state = unsafe { &mut *(ptr as *mut O) };
            O::invalidate(state);
        }

        let offset = observer as *const _ as isize - this as *const _ as isize;
        let invalidate: unsafe fn(*mut u8, &S) = invalidate::<O, S>;
        unsafe { &mut *this.states.get() }.push((offset, invalidate));
    }
}

impl<S: ?Sized> Deref for Pointer<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        unsafe { Self::as_ref(self) }
    }
}

impl<S: ?Sized> DerefMut for Pointer<S> {
    fn deref_mut(&mut self) -> &mut S {
        unsafe { Self::as_mut(self) }
    }
}

impl<S: ?Sized> Debug for Pointer<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Pointer").field(&self.inner.get()).finish()
    }
}

impl<S: ?Sized> PartialEq for Pointer<S> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<S: ?Sized> Eq for Pointer<S> {}
