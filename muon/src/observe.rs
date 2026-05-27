//! Types and traits for observing mutations to data structures.
//!
//! See the [Observer Mechanism](https://github.com/shigma/muon#observer-mechanism) section in
//! the README for a detailed overview of the observer architecture, dereference chains, and
//! mutation tracking primitives.

use crate::general::SnapshotObserver;
use crate::general::snapshot::Snapshot;
pub use crate::general::snapshot::SnapshotSpec;
use crate::helper::{AsDeref, AsDerefMut, Pointer, QuasiObserver, Unsigned, Zero};
use crate::{Adapter, Mutations};

/// A trait for observer types that wrap and track mutations to values.
///
/// Observers provide transparent access to the underlying value while recording any mutations that
/// occur. They form a dereference chain that allows multiple levels of observation.
///
/// ## Lifecycle
///
/// - [`observe(head)`](Self::observe) fully initializes the observer: sets up the internal pointer,
///   initializes diff state, and registers any fallback invalidation entries.
/// - [`relocate(this, head)`](Self::relocate) updates the internal pointer after the observed value
///   has moved in memory (e.g., due to [`Vec`] reallocation), keeping diff state intact.
///
/// ## Invariants
///
/// ### Inline-Field Invariant
///
/// Every [`Observer`]'s [`Deref`](std::ops::Deref) target must be an inline field (or nested
/// inline field) — no [`Box`], [`Arc`](std::sync::Arc), or other heap indirection in the deref
/// chain. This ensures that every field within the observer hierarchy has a **fixed byte offset**
/// relative to the [`Pointer<Head>`](Pointer), invariant under moves.
///
/// This property is required by [`Pointer`]'s fallback invalidation mechanism: any observer in
/// the deref chain can register sibling fields with the [`Pointer`] via [`Pointer::register_state`]
/// or [`Pointer::register_observer`] during [`observe`](Observer::observe). The [`Pointer`]
/// accumulates entries from all levels. When [`DerefMut`](std::ops::DerefMut) propagates down to
/// the tail observer, the tail calls [`Pointer::invalidate`](QuasiObserver::invalidate), which
/// iterates all registered `(offset, invalidate_fn)` entries to reach those siblings via
/// offset-based addressing — invalidating siblings across the entire chain in a single pass.
///
/// Since [`&mut Pointer<S>`](Pointer) only has provenance over the [`Pointer`] itself, the
/// offset-based addressing uses the [exposed-provenance](std::ptr#exposed-provenance) API. Every
/// observer that registers siblings must also call
/// [`expose_provenance`](pointer::expose_provenance) on `&mut self` in its
/// [`DerefMut`](std::ops::DerefMut) impl, depositing the parent struct's provenance into the
/// global pool.
///
/// ### Valid-State Invariant
///
/// [`QuasiObserver::invalidate`] must fully reset all granular tracking state and clear inner
/// observer storage (dropping or resetting inner observers). This ensures that subsequent
/// [`flush`](SerializeObserver::flush) calls cannot produce incorrect mutations from stale
/// tracking state, and that later accesses cannot obtain inner observers carrying stale state.
///
/// In contrast, a stale pointer (e.g., an inner observer pointing to a previous address after
/// container reallocation) is tolerable — it will be repaired by [`relocate`](Observer::relocate)
/// before the next access. Stale state, however, cannot be repaired after the fact, which is why
/// [`QuasiObserver::invalidate`] must eagerly clear it.
///
/// See the [Observer Mechanism](https://github.com/shigma/muon#observer-mechanism) for a
/// detailed overview of the dereference chain and mutation tracking primitives.
pub trait Observer: QuasiObserver<Target = Pointer<<Self as QuasiObserver>::Head>> + Sized {
    /// Creates a new observer for the given value.
    ///
    /// This is the primary way to create an observer. The observer will track all mutations to the
    /// provided value.
    ///
    /// ## Example
    ///
    /// ```
    /// use muon::general::ShallowObserver;
    /// use muon::observe::Observer;
    ///
    /// let mut value = 42;
    /// let observer = unsafe { ShallowObserver::<i32, i32>::observe(&mut value) };
    /// ```
    ///
    /// # Safety
    ///
    /// The caller must ensure that `head` is a valid pointer to the observed value.
    unsafe fn observe(head: *mut Self::Head) -> Self;

    /// Updates the observer's internal pointer after the observed value has moved.
    ///
    /// This method updates the observer's internal pointer to point to the new location
    /// of the observed value. It is necessary when the observed value is relocated in
    /// memory (e.g., due to [`Vec`] reallocation) while the observer remains active.
    ///
    /// ## Guarantee
    ///
    /// After `relocate` returns, the observer's internal [`Pointer`] must hold provenance
    /// compatible with `head`. This ensures that subsequent accesses through the pointer
    /// (e.g., via [`DerefMut`](std::ops::DerefMut)) remain valid.
    ///
    /// ## Safety
    ///
    /// The caller must ensure that `head` refers to the same logical value with which the
    /// observer was initialized, just potentially at a new memory location.
    unsafe fn relocate(this: &mut Self, head: *mut Self::Head);
}

/// Extends [`Observer`] with the ability to flush recorded mutations as serializable values.
///
/// This trait uses type-erased serialization: mutation values are stored as
/// [`Box<dyn erased_serde::Serialize>`](erased_serde::Serialize) and only serialized when an
/// [`Adapter`] converts them.
pub trait SerializeObserver: Observer {
    /// Extracts all recorded mutations and fully resets internal state.
    ///
    /// After calling `flush`, the observer's state is fully reset: an immediately subsequent
    /// `flush` with no intervening mutations must return empty. This invariant applies
    /// recursively to all nested observers and handler types.
    ///
    /// **Replace collapse**: If all inner fields or elements of a composite observer report
    /// [`Replace`](crate::MutationKind::Replace), the observer should collapse them into a
    /// single whole-value [`Replace`](crate::MutationKind::Replace). This applies to structs,
    /// tuples, arrays, and slices.
    fn flush(this: &mut Self) -> Mutations;

    /// Flushes mutations for a `#[serde(flatten)]` field.
    ///
    /// Returns a [`Mutations`] whose [`is_replace`](Mutations::is_replace) flag indicates whether
    /// the observer's entire content was replaced. When [`is_replace`](Mutations::is_replace) is
    /// true, the returned mutations contain per-field [`Replace`](crate::MutationKind::Replace)
    /// mutations (a flattened decomposition), not a single root-level
    /// [`Replace`](crate::MutationKind::Replace). This is the opposite of replace collapse:
    /// even when the whole value is replaced, the result is broken apart into individual field
    /// mutations so they can be merged into the parent's mutation set.
    ///
    /// The parent struct uses the [`is_replace`](Mutations::is_replace) flag to decide whether all
    /// of its fields (including this flattened one) were replaced, and if so, collapses
    /// everything into a single whole-struct [`Replace`](crate::MutationKind::Replace).
    ///
    /// The default implementation panics. Only struct observers (generated by the derive macro),
    /// map observers, and wrapper observers that delegate to an inner observer (e.g.,
    /// [`DerefObserver`](crate::impls::DerefObserver),
    /// [`CowObserver`](crate::impls::CowObserver),
    /// [`NewtypeObserver`](crate::impls::NewtypeObserver)) implement this method.
    fn flat_flush(_this: &mut Self) -> Mutations {
        panic!("flat_flush can only be called on structs and maps")
    }
}

/// Extension trait providing ergonomic methods for [`SerializeObserver`].
///
/// This trait is automatically implemented for all types that implement [`SerializeObserver`] and
/// provides convenient methods that don't require turbofish syntax.
pub trait SerializeObserverExt: SerializeObserver {
    /// Collects mutations using the specified adapter.
    ///
    /// This is a convenience method for [`SerializeObserver::flush`].
    fn flush<A: Adapter>(&mut self) -> Result<A, A::Error> {
        A::from_mutations(SerializeObserver::flush(self))
    }

    /// Collects flattened mutations using the specified adapter.
    ///
    /// This is a convenience method for [`SerializeObserver::flat_flush`].
    fn flat_flush<A: Adapter>(&mut self) -> Result<A, A::Error> {
        A::from_mutations(SerializeObserver::flat_flush(self))
    }
}

impl<T: SerializeObserver> SerializeObserverExt for T {}

/// Default observation specification.
///
/// [`DefaultSpec`] indicates that no special observation behavior is required for the type. For
/// most types, this means they use their standard [`Observer`] implementation. For example, if `T`
/// implements [`Observe`] with `Spec = DefaultSpec`, then [`Option<T>`] will be observed using
/// [`OptionObserver`](crate::impls::OptionObserver) which wraps `T`'s observer.
///
/// All `#[derive(Observe)]` implementations use [`DefaultSpec`] unless overridden with field
/// attributes.
pub struct DefaultSpec;

/// A trait for types that can be observed for mutations.
///
/// Types implementing [`Observe`] can be wrapped in [`Observer`]s that track mutations. The trait
/// is typically derived using the `#[derive(Observe)]` macro and used in `observe!` macros.
///
/// A single type `T` may have many possible [`Observer<'ob, Target = T>`] implementations in
/// theory, each with different change-tracking strategies. The [`Observe`] trait selects one
/// of these as the *default* observer to be used by `#[derive(Observe)]` and other generic code
/// that needs an observer for `T`.
///
/// When you `#[derive(Observe)]` on a struct, the macro requires that each field type
/// implements [`Observe`] so it can select an appropriate default observer for that field.
/// The [`Observer`] associated type of each field's [`Observe`] implementation determines which
/// observer will be instantiated in the generated code.
///
/// ## Example
///
/// ```
/// use muon::adapter::Json;
/// use muon::{Observe, observe};
/// use serde::Serialize;
///
/// #[derive(Serialize, Observe)]
/// struct MyStruct {
///     field: String,
/// }
///
/// let mut data = MyStruct { field: "value".to_string() };
/// let Json(mutation) = observe!(data => {
///     data.field.push_str(" modified");
/// }).unwrap();
/// ```
pub trait Observe {
    /// The default observer implementation for this type.
    type Observer<'ob, S, D>: Observer<Head = S, InnerDepth = D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    /// Marker type for selecting specialized observer implementations in wrapper types.
    ///
    /// For most types, this will be [`DefaultSpec`]. Types can specify [`SnapshotSpec`] to enable
    /// snapshot-based observation strategies. For example, [`Option<T>`] uses
    /// [`OptionObserver`](crate::impls::OptionObserver) when `T::Spec = DefaultSpec`, but
    /// [`SnapshotObserver`](crate::general::SnapshotObserver) when `T::Spec = SnapshotSpec`.
    type Spec;
}

/// Counterpart to [`Observe`] for shared-reference types.
///
/// A type `T` implements [`RoObserve`] if it can be observed through a shared reference (e.g.,
/// `&T`, [`Rc<T>`](std::rc::Rc), [`Arc<T>`](std::sync::Arc)).
///
/// See also: [`Observe`], [`RwObserve`].
pub trait RoObserve {
    /// The default observer implementation for `&Self`.
    type Observer<'ob, S, D>: Observer<Head = S, InnerDepth = D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    /// Marker type for selecting specialized observer implementations in wrapper types.
    ///
    /// For most types, this will be [`DefaultSpec`]. Types can specify [`SnapshotSpec`] to enable
    /// snapshot-based observation strategies. For example, [`Option<T>`] uses
    /// [`OptionObserver`](crate::impls::OptionObserver) when `T::Spec = DefaultSpec`, but
    /// [`SnapshotObserver`](crate::general::SnapshotObserver) when `T::Spec = SnapshotSpec`.
    type Spec;
}

/// Counterpart to [`Observe`] for interior-mutable types.
///
/// A type `T` implements [`RwObserve`] if it can be observed through interior mutability (e.g.,
/// [`RefCell<T>`](std::cell::RefCell), [`Mutex<T>`](std::sync::Mutex)).
///
/// A blanket implementation is provided for all types that implement [`Snapshot`], using
/// [`SnapshotObserver`] as the observer.
///
/// See also: [`Observe`], [`RoObserve`].
pub trait RwObserve {
    /// The default observer implementation for interior-mutable wrappers.
    type Observer<'ob, S, D>: Observer<Head = S, InnerDepth = D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    /// Marker type for selecting specialized observer implementations in wrapper types.
    type Spec;
}

impl<T: Snapshot> RwObserve for T {
    type Observer<'ob, S, D>
        = SnapshotObserver<'ob, Self, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = SnapshotSpec;
}

/// Extension trait providing ergonomic methods for types implementing [`Observe`].
///
/// This trait is automatically implemented for all types that implement [`Observe`] and provides a
/// convenient way to create observers without needing to specify type parameters.
///
/// ## Example
///
/// ```
/// use muon::observe::ObserveExt;
///
/// let mut data = 42;
/// let ob = data.__observe();
/// ```
pub trait ObserveExt: Observe {
    /// Creates an observer for this value.
    ///
    /// This is a convenience method that calls [`Observer::observe`] with the appropriate type
    /// parameters automatically inferred.
    fn __observe<'ob>(&'ob mut self) -> Self::Observer<'ob, Self, Zero> {
        unsafe { Observer::observe(self) }
    }
}

impl<T: Observe + ?Sized> ObserveExt for T {}

/// Resolves the concrete [`Observer`] type for a given [`Observe`] type.
///
/// This is a convenience alias used primarily by the derive macro to refer to field observer types
/// without repeating the full associated type syntax.
///
/// ## Type Parameters
///
/// - `T`: The observed type (must implement [`Observe`]).
/// - `S`: The head type stored in the observer's [`Pointer`]. Defaults to `T` (for top-level or
///   struct-field observers where the head is the field itself).
/// - `D`: The [`InnerDepth`](QuasiObserver::InnerDepth). Defaults to [`Zero`] (no extra dereference
///   layers between `S` and `T`).
pub type DefaultObserver<'ob, T, S = T, D = Zero> = <T as Observe>::Observer<'ob, S, D>;

/// Resolves the concrete [`Observer`] type for a given [`RoObserve`] type.
///
/// This is a convenience alias used primarily by the derive macro to refer to field observer types
/// without repeating the full associated type syntax.
///
/// ## Type Parameters
///
/// - `T`: The observed type (must implement [`RoObserve`]).
/// - `S`: The head type stored in the observer's [`Pointer`]. Defaults to `T` (for top-level or
///   struct-field observers where the head is the field itself).
/// - `D`: The [`InnerDepth`](QuasiObserver::InnerDepth). Defaults to [`Zero`] (no extra dereference
///   layers between `S` and `T`).
pub type DefaultRoObserver<'ob, T, S = T, D = Zero> = <T as RoObserve>::Observer<'ob, S, D>;

/// Resolves the concrete [`Observer`] type for a given [`RwObserve`] type.
///
/// ## Type Parameters
///
/// - `T`: The observed type (must implement [`RwObserve`]).
/// - `S`: The head type stored in the observer's [`Pointer`]. Defaults to `T`.
/// - `D`: The [`InnerDepth`](QuasiObserver::InnerDepth). Defaults to [`Zero`].
pub type DefaultRwObserver<'ob, T, S = T, D = Zero> = <T as RwObserve>::Observer<'ob, S, D>;
