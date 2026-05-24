//! Type-level representation of unsigned natural numbers.

use std::marker::PhantomData;

mod private {
    pub trait Sealed {}
}

/// A type-level representation of unsigned natural numbers.
///
/// This trait is implemented for types that represent natural numbers at the type level, enabling
/// compile-time arithmetic and recursion depth tracking.
///
/// Use [`Zero`] and [`Succ`] to construct type-level numbers.
pub trait Unsigned: private::Sealed + 'static {}

/// Type-level zero.
///
/// Represents the natural number 0 in the type system.
pub struct Zero;

impl private::Sealed for Zero {}
impl Unsigned for Zero {}

/// Type-level successor.
///
/// Represents the natural number `N + 1` where `N` is another [`Unsigned`].
///
/// ## Examples
///
/// - [`Zero`] = 0
/// - [`Succ<Zero>`] = 1
/// - [`Succ<Succ<Zero>>`] = 2
pub struct Succ<N>(PhantomData<N>);

impl<N: Unsigned> private::Sealed for Succ<N> {}
impl<N: Unsigned> Unsigned for Succ<N> {}
