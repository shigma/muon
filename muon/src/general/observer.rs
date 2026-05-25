use std::fmt::Debug;
use std::marker::PhantomData;
use std::num::NonZero;
use std::ops::{Deref, DerefMut};

use serde::Serialize;

use crate::Mutations;
use crate::helper::{AsDeref, AsDerefMut, AsDerefPtrExt, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{Observer, SerializeObserver};

/// A handler trait for implementing change detection strategies in [`GeneralObserver`].
///
/// [`GeneralHandler`] defines the interface for pluggable change detection strategies used
/// exclusively with [`GeneralObserver`]. Each handler implementation encapsulates a specific
/// approach to detecting whether a value has changed.
///
/// ## Example
///
/// A [`ShallowObserver`](super::ShallowObserver) implementation that treats any mutation through
/// [`DerefMut`] as a complete replacement:
///
/// ```
/// # use std::marker::PhantomData;
/// # use muon::general::{GeneralHandler, GeneralObserver};
/// # use muon::helper::Invalidate;
/// # use muon::observe::DefaultSpec;
/// struct ShallowHandler<T> {
///     mutated: bool,
///     phantom: PhantomData<T>,
/// }
///
/// impl<T> Invalidate<T> for ShallowHandler<T> {
///     fn invalidate(&mut self, _value: &T) {
///         self.mutated = true;
///     }
/// }
///
/// impl<T> GeneralHandler for ShallowHandler<T> {
///     type Target = T;
///     fn observe(_value: &T) -> Self {
///         Self { mutated: false, phantom: PhantomData }
///     }
/// }
///
/// type ShallowObserver<'ob, T> = GeneralObserver<'ob, T, ShallowHandler<T>>;
/// ```
pub trait GeneralHandler: Invalidate<Self::Target> {
    /// The observed value type that this handler tracks changes for.
    type Target: ?Sized;

    /// Implementation for [`Observer::observe`].
    fn observe(value: &Self::Target) -> Self;
}

/// A handler that can serialize mutations for [`GeneralObserver`].
///
/// This trait extends [`GeneralHandler`] with serialization capabilities. A [`GeneralHandler`]
/// must implement [`SerializeHandler`] for its corresponding [`GeneralObserver`] to implement
/// [`SerializeObserver`].
///
/// ## Blanket Implementation
///
/// A blanket implementation is provided for all types that implement [`ReplaceHandler`]
/// where the observed type implements [`Serialize`]. This automatically converts the
/// boolean result from [`is_replace`](ReplaceHandler::is_replace) into a
/// [`Replace`](crate::MutationKind::Replace) mutation when changes are detected.
///
/// Most handlers only need to implement [`ReplaceHandler`] to gain full serialization
/// support. Direct implementation of [`SerializeHandler`] is only necessary for handlers
/// that need to emit non-replace mutations (like [`Append`](crate::MutationKind::Append)).
pub trait SerializeHandler: GeneralHandler {
    /// Flushes and serializes all recorded mutations.
    ///
    /// ## Safety
    ///
    /// This method assumes the handler is constructed with [`observe`](GeneralHandler::observe).
    ///
    /// See also [`SerializeObserver::flush`].
    unsafe fn flush(&mut self, value: &Self::Target) -> Mutations;
}

/// A handler that can only express replace-style mutations.
///
/// This trait provides a simplified interface for handlers that only need to track whether the
/// observed value has changed, without distinguishing between different mutation kinds (like
/// [`Append`](crate::MutationKind::Append) or [`Truncate`](crate::MutationKind::Truncate)). Most
/// [`GeneralHandler`] implementations implement this trait rather than [`SerializeHandler`]
/// directly.
pub trait ReplaceHandler: GeneralHandler {
    /// Returns whether the next flush would produce a [`Replace`](crate::MutationKind::Replace)
    /// mutation.
    ///
    /// ## Safety
    ///
    /// This method assumes the handler is constructed with [`observe`](GeneralHandler::observe).
    ///
    /// See also [`SerializeObserver::flush`].
    unsafe fn is_replace(&self, value: &Self::Target) -> bool;
}

impl<H> SerializeHandler for H
where
    H: ReplaceHandler,
    H::Target: Serialize + 'static,
{
    unsafe fn flush(&mut self, value: &Self::Target) -> Mutations {
        let is_replace = unsafe { ReplaceHandler::is_replace(self, value) };
        *self = H::observe(value);
        if is_replace {
            Mutations::replace(value)
        } else {
            Mutations::new()
        }
    }
}

/// A helper trait for providing a custom name when formatting [`GeneralObserver`] with [`Debug`].
///
/// [`DebugHandler`] extends [`GeneralHandler`] by adding a [`NAME`](DebugHandler::NAME) constant
/// used as the type label in [`Debug`] output for [`GeneralObserver`].
///
/// ## Example
///
/// ```
/// # use std::marker::PhantomData;
/// use muon::general::{DebugHandler, GeneralHandler, GeneralObserver};
/// use muon::helper::Invalidate;
/// use muon::observe::Observer;
///
/// pub struct MyHandler<T>(PhantomData<T>);
///
/// impl<T> Invalidate<T> for MyHandler<T> {
///     fn invalidate(&mut self, _: &T) {}
/// }
///
/// impl<T> GeneralHandler for MyHandler<T> {
///     type Target = T;
///     fn observe(_value: &T) -> Self { Self(PhantomData) }
/// }
///
/// impl<T> DebugHandler for MyHandler<T> {
///     const NAME: &'static str = "MyObserver";
/// }
///
/// let mut value = 123;
/// let ob = unsafe { GeneralObserver::<MyHandler<i32>, i32>::observe(&mut value) };
/// println!("{:?}", ob); // prints: MyObserver(123)
/// ```
pub trait DebugHandler: GeneralHandler {
    /// The name displayed when formatting the observer with [`Debug`].
    const NAME: &'static str;
}

/// A general-purpose [`Observer`] implementation with extensible change detection strategies.
///
/// [`GeneralObserver`] provides a flexible framework for implementing different change detection
/// strategies through the [`GeneralHandler`] trait. It serves as the foundation for several
/// built-in observer types.
///
/// ## Capabilities and Limitations
///
/// [`GeneralObserver`] can:
/// - Detect whether a value has changed via [`DerefMut`]
/// - Produce [`Replace`](crate::MutationKind::Replace) mutations when changes are detected
///
/// [`GeneralObserver`] cannot:
/// - Track field-level changes or interior mutations within complex types
/// - Add specialized implementations for common traits (e.g. [`AddAssign`](std::ops::AddAssign))
///
/// For types that benefit from more sophisticated change tracking, muon provides specialized
/// observer implementations. These include built-in support for [`String`] and [`Vec`] (which can
/// track append operations), as well as custom observers generated by `#[derive(Observe)]` (which
/// can track field-level changes).
///
/// ## Built-in Implementations
///
/// The following observer types are built on [`GeneralObserver`]:
///
/// - [`ShallowObserver`](super::ShallowObserver) - Tracks any [`DerefMut`] access as a change
/// - [`NoopObserver`](super::NoopObserver) - Ignores all changes
/// - [`SnapshotObserver`](super::SnapshotObserver) - Compares cloned snapshots to detect changes
pub struct GeneralObserver<'ob, H, S: ?Sized, D = Zero> {
    ptr: Pointer<S>,
    handler: H,
    phantom: PhantomData<&'ob mut D>,
}

impl<'ob, H, S: ?Sized, D> Deref for GeneralObserver<'ob, H, S, D> {
    type Target = Pointer<S>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<'ob, H, S: ?Sized, D> DerefMut for GeneralObserver<'ob, H, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::ptr::from_mut(self).expose_provenance();
        Pointer::invalidate(&mut self.ptr);
        &mut self.ptr
    }
}

impl<'ob, H, S: ?Sized, D, T: ?Sized> QuasiObserver for GeneralObserver<'ob, H, S, D>
where
    S: AsDeref<D, Target = T>,
    H: GeneralHandler<Target = T>,
    D: Unsigned,
{
    type Head = S;
    type OuterDepth = Succ<Zero>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        H::invalidate(&mut this.handler, (*this.ptr).as_deref());
    }
}

impl<'ob, H, S: ?Sized, D, T: ?Sized> Observer for GeneralObserver<'ob, H, S, D>
where
    S: AsDeref<D, Target = T>,
    H: GeneralHandler<Target = T>,
    D: Unsigned,
{
    unsafe fn observe(head: *mut Self::Head) -> Self {
        unsafe {
            let this = Self {
                handler: H::observe(&*head.as_deref_ptr::<D>()),
                ptr: Pointer::new_unchecked(head),
                phantom: PhantomData,
            };
            Pointer::register_state::<_, D>(&this.ptr, &this.handler);
            this
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe { Pointer::set_unchecked(this, head) };
    }
}

impl<'ob, H, S: ?Sized, D, T: ?Sized> SerializeObserver for GeneralObserver<'ob, H, S, D>
where
    S: AsDeref<D, Target = T>,
    H: SerializeHandler<Target = T>,
    D: Unsigned,
{
    fn flush(this: &mut Self) -> Mutations {
        unsafe { this.handler.flush((*this.ptr).as_deref()) }
    }
}

macro_rules! impl_fmt {
    ($($trait:ident),* $(,)?) => {
        $(
            impl<'ob, H, S: ?Sized, D> std::fmt::$trait for GeneralObserver<'ob, H, S, D>
            where
                H: GeneralHandler<Target = S::Target>,
                S: AsDeref<D>,
                D: Unsigned,
                S::Target: std::fmt::$trait,
            {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    std::fmt::$trait::fmt(self.untracked_ref(), f)
                }
            }
        )*
    };
}

impl_fmt! {
    Binary,
    Display,
    LowerExp,
    LowerHex,
    Octal,
    Pointer,
    UpperExp,
    UpperHex,
}

impl<'ob, H, S: ?Sized, D, T: ?Sized> Debug for GeneralObserver<'ob, H, S, D>
where
    S: AsDeref<D, Target = T>,
    H: DebugHandler<Target = T>,
    D: Unsigned,
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(H::NAME).field(&self.untracked_ref()).finish()
    }
}

impl<'ob, H, S: ?Sized, D, I> std::ops::Index<I> for GeneralObserver<'ob, H, S, D>
where
    H: GeneralHandler<Target = S::Target>,
    S: AsDeref<D>,
    D: Unsigned,
    S::Target: std::ops::Index<I>,
{
    type Output = <S::Target as std::ops::Index<I>>::Output;

    fn index(&self, index: I) -> &Self::Output {
        self.untracked_ref().index(index)
    }
}

impl<'ob, H, S: ?Sized, D, I> std::ops::IndexMut<I> for GeneralObserver<'ob, H, S, D>
where
    S: AsDerefMut<D>,
    H: GeneralHandler<Target = S::Target>,
    D: Unsigned,
    S::Target: std::ops::IndexMut<I>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        self.tracked_mut().index_mut(index)
    }
}

impl<'ob, H1, H2, S1: ?Sized, S2: ?Sized, D1, D2> PartialEq<GeneralObserver<'ob, H2, S2, D2>>
    for GeneralObserver<'ob, H1, S1, D1>
where
    H1: GeneralHandler<Target = S1::Target>,
    H2: GeneralHandler<Target = S2::Target>,
    S1: AsDeref<D1>,
    S2: AsDeref<D2>,
    D1: Unsigned,
    D2: Unsigned,
    S1::Target: PartialEq<S2::Target>,
{
    fn eq(&self, other: &GeneralObserver<'ob, H2, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<'ob, H, S: ?Sized, D> Eq for GeneralObserver<'ob, H, S, D>
where
    H: GeneralHandler<Target = S::Target>,
    S: AsDeref<D>,
    D: Unsigned,
    S::Target: Eq,
{
}

impl<'ob, H1, H2, S1: ?Sized, S2: ?Sized, D1, D2> PartialOrd<GeneralObserver<'ob, H2, S2, D2>>
    for GeneralObserver<'ob, H1, S1, D1>
where
    H1: GeneralHandler<Target = S1::Target>,
    H2: GeneralHandler<Target = S2::Target>,
    S1: AsDeref<D1>,
    S2: AsDeref<D2>,
    D1: Unsigned,
    D2: Unsigned,
    S1::Target: PartialOrd<S2::Target>,
{
    fn partial_cmp(&self, other: &GeneralObserver<'ob, H2, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<'ob, H, S: ?Sized, D> Ord for GeneralObserver<'ob, H, S, D>
where
    H: GeneralHandler<Target = S::Target>,
    S: AsDeref<D>,
    D: Unsigned,
    S::Target: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

macro_rules! impl_ops_assign {
    ($($trait:ident => $method:ident),* $(,)?) => {
        $(
            impl<'ob, H, S: ?Sized, D, T: ?Sized, U> std::ops::$trait<U> for GeneralObserver<'ob, H, S, D>
            where
                S: AsDerefMut<D, Target = T>,
                H: GeneralHandler<Target = T>,
                D: Unsigned,
                T: std::ops::$trait<U>,
            {
                fn $method(&mut self, rhs: U) {
                    self.tracked_mut().$method(rhs);
                }
            }
        )*
    };
}

impl_ops_assign! {
    AddAssign => add_assign,
    SubAssign => sub_assign,
    MulAssign => mul_assign,
    DivAssign => div_assign,
    RemAssign => rem_assign,
    BitAndAssign => bitand_assign,
    BitOrAssign => bitor_assign,
    BitXorAssign => bitxor_assign,
    ShlAssign => shl_assign,
    ShrAssign => shr_assign,
}

macro_rules! impl_ops_copy {
    ($($trait:ident => $method:ident),* $(,)?) => {
        $(
            impl<'ob, H, S: ?Sized, D, T: ?Sized, U> std::ops::$trait<U> for GeneralObserver<'ob, H, S, D>
            where
                H: GeneralHandler<Target = T>,
                S: AsDeref<D, Target = T>,
                D: Unsigned,
                T: std::ops::$trait<U> + Copy,
            {
                type Output = <T as std::ops::$trait<U>>::Output;

                fn $method(self, rhs: U) -> Self::Output {
                    self.untracked_ref().$method(rhs)
                }
            }
        )*
    };
}

impl_ops_copy! {
    Add => add,
    Sub => sub,
    Mul => mul,
    Div => div,
    Rem => rem,
    BitAnd => bitand,
    BitOr => bitor,
    BitXor => bitxor,
    Shl => shl,
    Shr => shr,
}

macro_rules! impl_ops_copy_unary {
    ($($trait:ident => $method:ident),* $(,)?) => {
        $(
            impl<'ob, H, S: ?Sized, D, T: ?Sized> std::ops::$trait for GeneralObserver<'ob, H, S, D>
            where
                H: GeneralHandler<Target = T>,
                S: AsDeref<D, Target = T>,
                D: Unsigned,
                T: std::ops::$trait + Copy,
            {
                type Output = <T as std::ops::$trait>::Output;

                fn $method(self) -> Self::Output {
                    (*self.untracked_ref()).$method()
                }
            }
        )*
    }
}

impl_ops_copy_unary! {
    Neg => neg,
    Not => not,
}

macro_rules! impl_partial_eq {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<'ob, H, S: ?Sized, D> PartialEq<$ty> for GeneralObserver<'ob, H, S, D>
            where
                S: AsDeref<D, Target = $ty>,
                D: Unsigned,
            {
                fn eq(&self, other: &$ty) -> bool {
                    (***self).as_deref().eq(other)
                }
            }
        )*
    };
}

impl_partial_eq! {
    (), usize, u8, u16, u32, u64, u128, isize, i8, i16, i32, i64, i128, f32, f64, bool, char,
    NonZero<usize>, NonZero<u8>, NonZero<u16>, NonZero<u32>, NonZero<u64>, NonZero<u128>,
    NonZero<isize>, NonZero<i8>, NonZero<i16>, NonZero<i32>, NonZero<i64>, NonZero<i128>,
    core::net::IpAddr, core::net::Ipv4Addr, core::net::Ipv6Addr,
    core::net::SocketAddr, core::net::SocketAddrV4, core::net::SocketAddrV6,
    core::time::Duration, std::time::SystemTime,
}

#[cfg(feature = "chrono")]
impl_partial_eq! {
    chrono::Days, chrono::FixedOffset, chrono::Month, chrono::Months, chrono::IsoWeek,
    chrono::NaiveDate, chrono::NaiveDateTime, chrono::NaiveTime, chrono::NaiveWeek,
    chrono::TimeDelta, chrono::Utc, chrono::Weekday, chrono::WeekdaySet,
}

#[cfg(feature = "uuid")]
impl_partial_eq! {
    uuid::Uuid, uuid::NonNilUuid,
}

macro_rules! impl_partial_ord {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<'ob, H, S: ?Sized, D> PartialOrd<$ty> for GeneralObserver<'ob, H, S, D>
            where
                S: AsDeref<D, Target = $ty>,
                D: Unsigned,
            {
                fn partial_cmp(&self, other: &$ty) -> Option<std::cmp::Ordering> {
                    (***self).as_deref().partial_cmp(other)
                }
            }
        )*
    };
}

impl_partial_ord! {
    (), usize, u8, u16, u32, u64, u128, isize, i8, i16, i32, i64, i128, f32, f64, bool, char,
    NonZero<usize>, NonZero<u8>, NonZero<u16>, NonZero<u32>, NonZero<u64>, NonZero<u128>,
    NonZero<isize>, NonZero<i8>, NonZero<i16>, NonZero<i32>, NonZero<i64>, NonZero<i128>,
    core::net::IpAddr, core::net::Ipv4Addr, core::net::Ipv6Addr,
    core::net::SocketAddr, core::net::SocketAddrV4, core::net::SocketAddrV6,
    core::time::Duration, std::time::SystemTime,
}

#[cfg(feature = "chrono")]
impl_partial_ord! {
    chrono::Days, chrono::Month, chrono::Months, chrono::IsoWeek,
    chrono::NaiveDate, chrono::NaiveDateTime, chrono::NaiveTime,
    chrono::TimeDelta, chrono::WeekdaySet,
}

#[cfg(feature = "uuid")]
impl_partial_ord! {
    uuid::Uuid,
}

macro_rules! generic_impl_cmp {
    ($(impl $([$($gen:tt)*])? _ for $ty:ty);* $(;)?) => {
        $(
            impl<'ob, $($($gen)*,)? H, S: ?Sized, D> PartialEq<$ty> for GeneralObserver<'ob, H, S, D>
            where
                S: AsDeref<D>,
                D: Unsigned,
                S::Target: PartialEq<$ty>,
            {
                fn eq(&self, other: &$ty) -> bool {
                    (***self).as_deref().eq(other)
                }
            }

            impl<'ob, $($($gen)*,)? H, S: ?Sized, D> PartialOrd<$ty> for GeneralObserver<'ob, H, S, D>
            where
                S: AsDeref<D>,
                D: Unsigned,
                S::Target: PartialOrd<$ty>,
            {
                fn partial_cmp(&self, other: &$ty) -> Option<std::cmp::Ordering> {
                    (***self).as_deref().partial_cmp(other)
                }
            }
        )*
    };
}

generic_impl_cmp! {
    impl [U] _ for std::marker::PhantomData<U>;
    impl ['a, U] _ for &'a [U];
}

#[cfg(feature = "chrono")]
generic_impl_cmp! {
    impl [Tz: chrono::TimeZone] _ for chrono::DateTime<Tz>;
}
