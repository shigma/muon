//! Helper utilities for internal implementation details.
//!
//! This module contains traits and types that support muon's internal machinery.
//!
//! ## Contents
//!
//! - [`Unsigned`], [`Zero`], [`Succ`] - Type-level natural numbers for compile-time depth tracking
//! - [`AsDeref`], [`AsDerefMut`] - Inductive recursive dereferencing
//! - [`AsDerefCoinductive`], [`AsDerefMutCoinductive`] - Coinductive recursive dereferencing
//! - [`QuasiObserver`] - Enables consistent operations between observers and normal references via
//!   autoref-based specialization
//! - [`Pointer`] - Internal pointer type for observer dereference chains

pub mod deref;
pub(crate) mod macros;
mod pointer;
pub mod quasi;
pub mod shallow;
pub mod unsigned;

pub use deref::{AsDeref, AsDerefCoinductive, AsDerefMut, AsDerefMutCoinductive};
pub use pointer::Pointer;
pub use quasi::{Invalidate, QuasiObserver};
pub use unsigned::{Succ, Unsigned, Zero};
#[doc(hidden)]
pub use {erased_serde, serde};
