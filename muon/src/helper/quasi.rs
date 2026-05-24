//! [`QuasiObserver`] trait, [`Invalidate`] trait, and autoref-based specialization for the
//! [`observe!`](crate::observe!) macro.
//!
//! The [`observe!`](crate::observe!) macro transforms assignments (`lhs = rhs`) and comparisons
//! (`lhs == rhs`) into calls to [`tracked_mut`](QuasiObserver::tracked_mut) and
//! [`untracked_ref`](QuasiObserver::untracked_ref). These methods are implemented for both
//! observer types and plain references (`&T`, `&mut T`), so Rust's autoref-based method resolution
//! selects the correct implementation depending on whether the operand is an observer or a plain
//! value.
//!
//! [`Invalidate`] is the companion trait for types that carry internal tracking state (diff
//! trackers, inner observer containers). It provides a single
//! [`invalidate`](Invalidate::invalidate) entry point used by the fallback invalidation
//! mechanism in [`Pointer`].
//!
//! See the [Observer Mechanism](https://github.com/shigma/muon#observer-mechanism) section in
//! the README for a detailed overview.

use std::ops::{Deref, DerefMut};

use crate::helper::{AsDeref, AsDerefMut, AsDerefMutCoinductive, Pointer, Unsigned, Zero};

/// Enables [`tracked_mut`](QuasiObserver::tracked_mut) and
/// [`untracked_mut`](QuasiObserver::untracked_mut) to reach the [`Pointer`] without triggering
/// [`DerefMut`] on any observer layer.
///
/// The default implementation falls through to [`DerefMut`]. The key specialization is for
/// [`Pointer<S>`](Pointer), which uses immutable coinductive traversal followed by unsafe
/// interior-mutable access, bypassing all [`DerefMut`] hooks.
pub trait DerefMutUntracked: DerefMut {
    /// Traverses the coinductive dereference chain to reach the underlying value without triggering
    /// any observer [`DerefMut`] hooks (if possible).
    fn deref_mut_untracked<'a, U, D>(this: &'a mut U) -> &'a mut Self::Target
    where
        Self: 'a,
        D: Unsigned,
        U: AsDerefMutCoinductive<D, Target = Self> + ?Sized,
    {
        this.as_deref_mut_coinductive().deref_mut()
    }
}

impl<T: ?Sized> DerefMutUntracked for &mut T {}

impl<S: ?Sized> DerefMutUntracked for Pointer<S> {
    fn deref_mut_untracked<'a, U, D>(this: &'a mut U) -> &'a mut Self::Target
    where
        Self: 'a,
        D: Unsigned,
        U: AsDerefMutCoinductive<D, Target = Self> + ?Sized,
    {
        unsafe { Pointer::as_mut(this.as_deref_coinductive()) }
    }
}

/// Formalizes the dereference chain and provides the three mutation tracking primitives:
/// [`untracked_ref`](Self::untracked_ref), [`untracked_mut`](Self::untracked_mut), and
/// [`tracked_mut`](Self::tracked_mut).
///
/// Also implemented for `&T`, `&mut T`, and [`Pointer<T>`] (where all methods reduce to identity),
/// enabling the [`observe!`](crate::observe!) macro to work uniformly with both observers and plain
/// references via autoref-based specialization. The name "quasi-observer" reflects this dual
/// nature — plain references are not real observers, but they participate in the same interface.
///
/// ## Primitives of Mutation Tracking
///
/// | Method                                 | Receiver    | Triggers Invalidation |
/// | -------------------------------------- | ----------- | --------------------- |
/// | [`untracked_ref`](Self::untracked_ref) | `&self`     | No                    |
/// | [`untracked_mut`](Self::untracked_mut) | `&mut self` | No                    |
/// | [`tracked_mut`](Self::tracked_mut)     | `&mut self` | Yes                   |
///
/// ## Dereference Chain
///
/// A `QuasiObserver` defines a two-segment dereference chain:
///
/// ```text
/// Self --[OuterDepth]-> Pointer<Head> --> Head --[InnerDepth]-> Target
///        (coinductive)                           (inductive)
/// ```
///
/// - [`OuterDepth`](QuasiObserver::OuterDepth): The number of coinductive dereferences from `Self`
///   to its internal [`Pointer`]. For a simple observer this is `Succ<Zero>` (one). For a composite
///   observer like [`VecObserver`](crate::impls::VecObserver) which wraps
///   [`SliceObserver`](crate::impls::SliceObserver), it is `Succ<Succ<Zero>>` (two). For `&T`,
///   `&mut T`, and [`Pointer<T>`] it is `Zero`.
///
/// - [`InnerDepth`](QuasiObserver::InnerDepth): The number of inductive dereferences from the
///   [`Head`](Self::Head) type (stored inside the [`Pointer`]) to the final observed type. For
///   example, when observing a [`Vec<T>`], the [`Head`](Self::Head) is [`Vec<T>`] and the observed
///   type is `[T]`, so `InnerDepth = Succ<Zero>`.
///
/// ## Implementation Notes
///
/// 1. **Every type implementing [`Observer`](crate::observe::Observer) should manually implement
///    [`QuasiObserver`]**. Without this implementation, assignments and comparisons in the
///    [`observe!`](crate::observe!) macro may not work as expected, potentially causing compilation
///    errors or incorrect behavior. We cannot provide a blanket implementation `impl<T: Observer>
///    QuasiObserver for T` because it would conflict with the `impl<T> QuasiObserver for &T` and
///    `impl<T> QuasiObserver for &mut T` implementations.
///
/// 2. **Do not implement [`QuasiObserver`] for types other than `&T`, `&mut T`, [`Pointer<T>`], and
///    [`Observer`](crate::observe::Observer) types**. Implementing [`QuasiObserver`] for other
///    [`Deref`] types (like [`Box`], [`MutexGuard`](std::sync::MutexGuard), etc.) may cause
///    unexpected behavior in the [`observe!`](crate::observe!) macro, as it would interfere with
///    the autoref-based specialization mechanism.
pub trait QuasiObserver: AsDerefMutCoinductive<Self::OuterDepth, Target: Deref<Target = Self::Head>> {
    /// The type stored inside the [`Pointer`], from which the inductive dereference chain begins.
    type Head: AsDeref<Self::InnerDepth> + ?Sized;

    /// The number of coinductive dereferences from `Self` to its internal [`Pointer`].
    type OuterDepth: Unsigned;

    /// The number of inductive dereferences from [`Head`](Self::Head) to the final observed type.
    type InnerDepth: Unsigned;

    /// Resets all granular tracking state in this observer.
    ///
    /// Called by [`tracked_mut`](Self::tracked_mut) before traversing the dereference chain, and
    /// by parent observers to cascade invalidation to their children. After this call, the next
    /// flush should produce a [`Replace`](crate::MutationKind::Replace) mutation.
    ///
    /// For plain references (`&T`, `&mut T`) and [`Pointer<T>`], this is a no-op. For observers,
    /// it delegates to [`Invalidate::invalidate`] on the internal tracking state and recursively
    /// invalidates child observers.
    fn invalidate(this: &mut Self);

    /// Returns an immutable reference to the observed value.
    ///
    /// The [`observe!`](crate::observe!) macro calls this method on both sides of comparison
    /// operators.
    fn untracked_ref<T: ?Sized>(&self) -> &T
    where
        Self::Head: AsDeref<Self::InnerDepth, Target = T>,
    {
        self.as_deref_coinductive().deref().as_deref()
    }

    /// Returns a mutable reference to the observed value **without** triggering any invalidation.
    ///
    /// Unlike [`tracked_mut`](QuasiObserver::tracked_mut), this method does not call
    /// [`invalidate`](Self::invalidate) and directly traverses the dereference chain to reach the
    /// underlying value, bypassing all [`DerefMut`] hooks to avoid every invalidation.
    ///
    /// ## Example
    ///
    /// Implementing [`Vec::pop`] for a [`VecObserver`](crate::impls::VecObserver):
    ///
    /// ```ignore
    /// impl VecObserver {
    ///     pub fn pop(&mut self) -> Option<T> {
    ///         if self.as_deref().len() > self.initial_len() {
    ///             // If the current length exceeds the initial length, the pop operation can be
    ///             // expressed by `MutationKind::Append`, so we do not trigger full mutation.
    ///             self.untracked_mut().pop()
    ///         } else {
    ///             // Otherwise, we need to treat the pop operation as `MutationKind::Replace`.
    ///             self.tracked_mut().pop()
    ///         }
    ///     }
    /// }
    /// ```
    fn untracked_mut<T: ?Sized>(&mut self) -> &mut T
    where
        Self::Target: DerefMutUntracked,
        Self::Head: AsDerefMut<Self::InnerDepth, Target = T>,
    {
        DerefMutUntracked::deref_mut_untracked(self).as_deref_mut()
    }

    /// Returns a mutable reference to the observed value.
    ///
    /// It first calls [`invalidate`](Self::invalidate) to reset all granular tracking state, then
    /// dereferences through the observer chain to reach the underlying value, bypassing all
    /// [`DerefMut`] hooks to avoid fallback invalidation.
    ///
    /// The [`observe!`](crate::observe!) macro calls this method on the left-hand side of
    /// assignment operators.
    fn tracked_mut<T: ?Sized>(&mut self) -> &mut T
    where
        Self::Target: DerefMutUntracked,
        Self::Head: AsDerefMut<Self::InnerDepth, Target = T>,
    {
        Self::invalidate(self);
        DerefMutUntracked::deref_mut_untracked(self).as_deref_mut()
    }
}

impl<T: ?Sized> QuasiObserver for &T {
    type Head = T;
    type OuterDepth = Zero;
    type InnerDepth = Zero;

    fn invalidate(_: &mut Self) {}
}

impl<T: ?Sized> QuasiObserver for &mut T {
    type Head = T;
    type OuterDepth = Zero;
    type InnerDepth = Zero;

    fn invalidate(_: &mut Self) {}
}

impl<T: ?Sized> QuasiObserver for Pointer<T> {
    type Head = T;
    type OuterDepth = Zero;
    type InnerDepth = Zero;

    /// Iterates all registered `(offset, invalidate_fn)` entries to invalidate sibling fields.
    ///
    /// Since [`&mut Pointer<S>`](Pointer) only has provenance over the [`Pointer`] itself,
    /// computing `base + offset` directly would produce a pointer whose provenance doesn't cover
    /// the sibling field (UB under Stacked/Tree Borrows). To solve this, every observer that
    /// registers siblings calls [`expose_provenance`](pointer::expose_provenance) on `&mut self`
    /// in its [`DerefMut`] impl, depositing the parent struct's provenance into the global pool.
    /// This method then uses [`with_exposed_provenance_mut`](std::ptr::with_exposed_provenance_mut)
    /// to reconstruct a pointer at each computed address, picking up the previously exposed
    /// provenance that covers both the [`Pointer`] and the sibling field.
    fn invalidate(this: &mut Self) {
        // Compute base_addr BEFORE creating a shared reference to states.
        // `from_mut(this)` creates a Unique retag that would invalidate any prior SharedReadOnly
        // tag on `states`, so the ordering matters for Stacked Borrows correctness.
        let base_addr = std::ptr::from_mut(this).addr();
        let states = unsafe { &*this.states.get() };
        let value = unsafe { Self::as_ref(this) };
        for &(offset, invalidate) in states {
            let target = std::ptr::with_exposed_provenance_mut::<u8>(base_addr.wrapping_add_signed(offset));
            unsafe { invalidate(target, value) }
        }
    }
}

/// Invalidation hook for state types that track an observed value.
///
/// Registered with [`Pointer::register_state`] for fallback invalidation. The method is named
/// `invalidate` rather than `mark_replace` to avoid coupling with
/// [`MutationKind`](crate::MutationKind).
///
/// The type parameter `T` represents the observed value type. `Invalidate<()>` represents
/// "blind" invalidation without access to the current value.
pub trait Invalidate<T: ?Sized> {
    /// Invalidates all granular tracking state. The next flush should produce a
    /// [`Replace`](crate::MutationKind::Replace) mutation.
    ///
    /// The post-invalidation state is **not** the "initial" state (which would be the clean state
    /// right after `observe`), but rather a state that signals "all granular tracking is lost."
    fn invalidate(&mut self, value: &T);
}
