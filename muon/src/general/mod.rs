//! General observation strategies.
//!
//! ## Usage
//!
//! Most users will interact with this module through attributes like `#[muon(shallow)]` for
//! field-level control. Direct use of types from this module is typically only needed for advanced
//! use cases.

mod noop;
mod observer;
mod pointer;
mod shallow;
pub(crate) mod snapshot;
mod unsize;

pub use noop::NoopObserver;
pub use observer::{DebugHandler, GeneralHandler, GeneralObserver, ReplaceHandler, SerializeHandler};
pub use pointer::PointerObserver;
pub use shallow::ShallowObserver;
pub use snapshot::{Snapshot, SnapshotObserver};
pub(crate) use unsize::{Unsize, UnsizeObserver};
