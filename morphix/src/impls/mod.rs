//! Observer implementations for library types.
//!
//! This module provides specialized [`Observer`](crate::observe::Observer) implementations
//! for common library types. These observers enable precise mutation tracking tailored to each
//! type's semantics.
//!
//! ## Usage
//!
//! These observers are typically used automatically through the [`Observe`](crate::Observe)
//! trait implementations. Direct usage is rarely needed unless implementing custom observers
//! or implementing foreign traits on observer types.
//!
//! ## Stability
//!
//! The internal module structure is not part of the public API and may change in future versions
//! without notice. Only items re-exported at the crate root or from this module are considered
//! stable.

mod atomic;
mod bound;
mod collections;
mod cow;
mod deref;
mod newtype;
mod option;
mod range;
mod slices;
mod strings;
mod tuple;
mod weak;

pub use bound::BoundObserver;
pub use collections::*;
pub use cow::CowObserver;
pub use deref::{DerefMutObserver, DerefObserver};
pub use newtype::NewtypeObserver;
pub use option::OptionObserver;
pub use slices::*;
pub use strings::*;
pub use tuple::{
    TupleObserver, TupleObserver2, TupleObserver3, TupleObserver4, TupleObserver5, TupleObserver6, TupleObserver7,
    TupleObserver8, TupleObserver9, TupleObserver10, TupleObserver11, TupleObserver12,
};
pub use weak::WeakObserver;
