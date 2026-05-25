//! Observer implementations for library types.
//!
//! This module provides specialized [`Observer`](crate::observe::Observer) implementations for
//! common library types. These observers enable precise mutation tracking tailored to each type's
//! semantics.
//!
//! ## Usage
//!
//! These observers are typically used automatically through the [`Observe`](crate::Observe) trait
//! implementations. Direct usage is rarely needed unless implementing custom observers or
//! implementing foreign traits on observer types.
//!
//! ## Stability
//!
//! The internal module structure is not part of the public API and may change in future versions
//! without notice. Only items re-exported at the crate root or from this module are considered
//! stable.

mod atomic;
mod collections;
mod slices;
mod strings;
mod wrappers;

pub use collections::*;
pub use slices::*;
pub use strings::*;
pub use wrappers::*;
