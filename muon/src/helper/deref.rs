//! Traits for recursive dereferencing with type-level natural numbers.
//!
//! This module provides two pairs of traits for expressing "can be dereferenced N times":
//! - [`AsDeref`] / [`AsDerefMut`]: Inductive version
//! - [`AsDerefCoinductive`] / [`AsDerefMutCoinductive`]: Coinductive version
//!
//! ## Inductive vs. Coinductive
//!
//! The key difference lies in their induction direction:
//!
//! - **Inductive**: If `T` can be dereferenced `N` times to reach a type that implements [`Deref`],
//!   then `T` can be dereferenced `N + 1` times.
//! - **Coinductive**: If `T` implements [`Deref`] to reach a type that can be dereferenced `N`
//!   times, then `T` can be dereferenced `N + 1` times.
//!
//! While these definitions are mathematically equivalent, Rust's type system cannot simply
//! recognize this equivalence. Implementing both patterns would cause conflicts, so we provide
//! separate traits. Choose the appropriate trait based on your actual induction direction in the
//! code.
//!
//! ## Type-level Natural Numbers
//!
//! These traits use [`Zero`] and [`Succ`] to represent the depth of dereferencing at the type
//! level, enabling compile-time verification of dereference chains.

use std::ops::{Deref, DerefMut};

use crate::helper::unsigned::{Succ, Unsigned, Zero};

/// Trait for types that can be dereferenced `N` times (inductive version).
///
/// See the [module documentation](self) for details about inductive vs. coinductive.
pub trait AsDeref<N: Unsigned> {
    /// The target type after `N` dereferences.
    type Target: ?Sized;

    /// Dereferences self `N` times.
    fn as_deref(&self) -> &Self::Target;
}

/// Trait for types that can be mutably dereferenced `N` times (inductive version).
///
/// See the [module documentation](self) for details about inductive vs. coinductive.
pub trait AsDerefMut<N: Unsigned>: AsDeref<N> {
    /// Mutably dereferences self `N` times.
    fn as_deref_mut(&mut self) -> &mut Self::Target;
}

impl<T: ?Sized> AsDeref<Zero> for T {
    type Target = T;

    fn as_deref(&self) -> &T {
        self
    }
}

impl<T: ?Sized> AsDerefMut<Zero> for T {
    fn as_deref_mut(&mut self) -> &mut T {
        self
    }
}

impl<T: AsDeref<N, Target: Deref> + ?Sized, N: Unsigned> AsDeref<Succ<N>> for T {
    type Target = <T::Target as Deref>::Target;

    fn as_deref(&self) -> &Self::Target {
        self.as_deref().deref()
    }
}

impl<T: AsDerefMut<N, Target: DerefMut> + ?Sized, N: Unsigned> AsDerefMut<Succ<N>> for T {
    fn as_deref_mut(&mut self) -> &mut Self::Target {
        self.as_deref_mut().deref_mut()
    }
}

/// Trait for types that can be dereferenced `N` times (coinductive version).
///
/// See the [module documentation](self) for details about inductive vs. coinductive.
pub trait AsDerefCoinductive<N: Unsigned> {
    /// The target type after `N` dereferences.
    type Target: ?Sized;

    /// Dereferences self `N` times.
    fn as_deref_coinductive(&self) -> &Self::Target;
}

/// Trait for types that can be mutably dereferenced `N` times (coinductive version).
///
/// See the [module documentation](self) for details about inductive vs. coinductive.
pub trait AsDerefMutCoinductive<N: Unsigned>: AsDerefCoinductive<N> {
    /// Mutably dereferences self `N` times.
    fn as_deref_mut_coinductive(&mut self) -> &mut Self::Target;
}

impl<T: ?Sized> AsDerefCoinductive<Zero> for T {
    type Target = T;

    fn as_deref_coinductive(&self) -> &T {
        self
    }
}

impl<T: ?Sized> AsDerefMutCoinductive<Zero> for T {
    fn as_deref_mut_coinductive(&mut self) -> &mut T {
        self
    }
}

impl<T: Deref<Target: AsDerefCoinductive<N>> + ?Sized, N: Unsigned> AsDerefCoinductive<Succ<N>> for T {
    type Target = <T::Target as AsDerefCoinductive<N>>::Target;

    fn as_deref_coinductive(&self) -> &Self::Target {
        self.deref().as_deref_coinductive()
    }
}

impl<T: DerefMut<Target: AsDerefMutCoinductive<N>> + ?Sized, N: Unsigned> AsDerefMutCoinductive<Succ<N>> for T {
    fn as_deref_mut_coinductive(&mut self) -> &mut Self::Target {
        self.deref_mut().as_deref_mut_coinductive()
    }
}
