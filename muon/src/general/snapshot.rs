use std::marker::PhantomData;
use std::num::NonZero;

use crate::general::{DebugHandler, GeneralHandler, GeneralObserver, SerializeHandler};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Unsigned, Zero};
use crate::observe::RoObserve;
use crate::{Mutations, Observe};

/// A general observer that uses snapshot comparison to detect actual value changes.
///
/// [`SnapshotObserver`] creates a clone of the initial value and compares it with the
/// final value using [`PartialEq`]. This provides accurate change detection by comparing
/// actual values rather than tracking access patterns.
///
/// ## Requirements
///
/// The observed type must implement:
/// - [`Clone`] - for creating the snapshot
/// - [`PartialEq`] - for comparing values
///
/// ## Derive Usage
///
/// Can be used via the `#[muon(snapshot)]` attribute in derive macros:
///
/// ```
/// # use muon::Observe;
/// # use serde::Serialize;
/// # #[derive(Serialize, Observe)]
/// # struct Uuid;
/// # #[derive(Serialize, Observe)]
/// # struct BitFlags;
/// #[derive(Serialize, Observe)]
/// struct MyStruct {
///     #[muon(snapshot)]
///     id: Uuid,           // Cheap to clone and compare
///     #[muon(snapshot)]
///     flags: BitFlags,    // Small Copy type
/// }
/// ```
///
/// ## When to Use
///
/// [`SnapshotObserver`] is ideal when:
/// 1. The type implements [`Clone`] and [`PartialEq`] with low cost
/// 2. Values may be modified and then restored to original (so that
///    [`ShallowObserver`](super::ShallowObserver) would yield false positives)
///
/// ## Built-in Usage
///
/// All scalar types that are [`Copy`] + [`PartialEq`] use [`SnapshotObserver`] as their default
/// implementation. This includes numeric primitives ([`i32`], [`f64`], [`bool`], etc.),
/// [`NonZero`](std::num::NonZero) variants, network types ([`IpAddr`](core::net::IpAddr),
/// [`SocketAddr`](core::net::SocketAddr)), and time types ([`Duration`](core::time::Duration),
/// [`SystemTime`](std::time::SystemTime)).
pub type SnapshotObserver<'ob, S, D = Zero> = GeneralObserver<'ob, SnapshotHandler<<S as AsDeref<D>>::Target>, S, D>;

/// A trait for creating snapshots of observable values.
///
/// [`Snapshot`] is used by [`SnapshotObserver`](crate::general::SnapshotObserver) to capture the
/// initial state of a value. The companion trait [`SerializeSnapshot`] handles comparison and
/// mutation production during flush.
///
/// ## Deep Copy Semantics
///
/// For most simple types, [`Snapshot`](Snapshot::Snapshot) is the type itself (i.e., `type Snapshot
/// = Self`). However, for pointer types like [`Rc<T>`](std::rc::Rc), [`&T`](reference), and
/// [`&mut T`](reference), the associated [`Snapshot`](Snapshot::Snapshot) type is `T::Snapshot`
/// rather than `Self`. This means [`Snapshot`] performs a "deep copy" through indirections,
/// capturing the underlying value rather than the pointer itself.
pub trait Snapshot {
    /// The snapshot type used for comparison.
    ///
    /// For value types, this is typically `Self`. For pointer and reference types, this is the
    /// snapshot type of the pointed-to value.
    type Snapshot;

    /// Creates a snapshot of the current value.
    ///
    /// For pointer types, this performs a deep copy of the underlying value.
    fn to_snapshot(&self) -> Self::Snapshot;
}

/// Extends [`Snapshot`] with the ability to flush recorded mutations by comparing against
/// a stored snapshot.
///
/// The `flush` method compares the current value against the old snapshot and produces
/// [`Mutations`]. The snapshot has already been updated (re-observed) before `flush` is called;
/// `flush` only needs to compare and return mutations.
pub trait SerializeSnapshot: Snapshot + serde::Serialize {
    /// Compares the current value against the old snapshot and produces mutations.
    fn flush(&self, snapshot: Self::Snapshot) -> Mutations;
}

pub struct SnapshotHandler<T: Snapshot + ?Sized> {
    snapshot: T::Snapshot,
    phantom: PhantomData<T>,
}

impl<T: Snapshot + ?Sized> Invalidate<T> for SnapshotHandler<T> {
    fn invalidate(&mut self, _: &T) {}
}

impl<T: Snapshot + ?Sized> GeneralHandler for SnapshotHandler<T> {
    type Target = T;

    fn observe(value: &T) -> Self {
        Self {
            snapshot: value.to_snapshot(),
            phantom: PhantomData,
        }
    }
}

impl<T: SerializeSnapshot + ?Sized> SerializeHandler for SnapshotHandler<T> {
    fn flush(&mut self, value: &T) -> Mutations {
        SerializeSnapshot::flush(value, std::mem::replace(&mut self.snapshot, value.to_snapshot()))
    }
}

impl<T: Snapshot + ?Sized> DebugHandler for SnapshotHandler<T> {
    const NAME: &'static str = "SnapshotObserver";
}

/// Snapshot-based observation specification.
///
/// [`SnapshotSpec`] marks a type as supporting efficient snapshot comparison (requires [`Clone`] +
/// [`PartialEq`]). When used as the [`Spec`](crate::Observe::Spec) for a type `T`, it affects
/// certain wrapper type observations, such as [`Option<T>`].
pub struct SnapshotSpec;

macro_rules! impl_snapshot_observe {
    ($($ty:ty),* $(,)?) => {
        $(
            impl Snapshot for $ty {
                type Snapshot = Self;
                fn to_snapshot(&self) -> Self {
                    *self
                }
            }

            impl SerializeSnapshot for $ty {
                fn flush(&self, snapshot: Self) -> Mutations {
                    if self != &snapshot {
                        Mutations::replace(self)
                    } else {
                        Mutations::new()
                    }
                }
            }

            impl Observe for $ty {
                type Observer<'ob, S, D>
                    = SnapshotObserver<'ob, S, D>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

                type Spec = SnapshotSpec;
            }

            impl RoObserve for $ty {
                type Observer<'ob, S, D>
                    = SnapshotObserver<'ob, S, D>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDeref<D, Target = Self> + ?Sized + 'ob;

                type Spec = SnapshotSpec;
            }
        )*
    };
}

impl_snapshot_observe! {
    (), usize, u8, u16, u32, u64, u128, isize, i8, i16, i32, i64, i128, f32, f64, bool, char,
    NonZero<usize>, NonZero<u8>, NonZero<u16>, NonZero<u32>, NonZero<u64>, NonZero<u128>,
    NonZero<isize>, NonZero<i8>, NonZero<i16>, NonZero<i32>, NonZero<i64>, NonZero<i128>,
    core::net::IpAddr, core::net::Ipv4Addr, core::net::Ipv6Addr,
    core::net::SocketAddr, core::net::SocketAddrV4, core::net::SocketAddrV6,
    core::time::Duration, std::time::SystemTime,
}

#[cfg(feature = "chrono")]
impl_snapshot_observe! {
    chrono::Month, chrono::NaiveDate, chrono::NaiveDateTime,
    chrono::NaiveTime, chrono::TimeDelta, chrono::Weekday,
}

#[cfg(feature = "uuid")]
impl_snapshot_observe! {
    uuid::Uuid, uuid::NonNilUuid,
}

macro_rules! generic_impl_snapshot_observe {
    ($(impl $([$($gen:tt)*])? _ for $ty:ty);* $(;)?) => {
        $(
            impl<$($($gen)*)?> Snapshot for $ty {
                type Snapshot = Self;
                fn to_snapshot(&self) -> Self {
                    self.clone()
                }
            }

            impl<$($($gen)*)?> SerializeSnapshot for $ty where Self: serde::Serialize {
                fn flush(&self, snapshot: Self) -> Mutations {
                    if self != &snapshot {
                        Mutations::replace(self)
                    } else {
                        Mutations::new()
                    }
                }
            }

            impl<$($($gen)*)?> Observe for $ty {
                type Observer<'ob, S, D>
                    = SnapshotObserver<'ob, S, D>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

                type Spec = SnapshotSpec;
            }

            impl<$($($gen)*)?> RoObserve for $ty {
                type Observer<'ob, S, D>
                    = SnapshotObserver<'ob, S, D>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDeref<D, Target = Self> + ?Sized + 'ob;

                type Spec = SnapshotSpec;
            }
        )*
    };
}

generic_impl_snapshot_observe! {
    impl [T] _ for std::marker::PhantomData<T>;
}

#[cfg(feature = "chrono")]
generic_impl_snapshot_observe! {
    impl [Tz: chrono::TimeZone] _ for chrono::DateTime<Tz>;
}
