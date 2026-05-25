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

use std::borrow::Cow;
use std::ffi::{CString, OsString};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use crate::helper::unsigned::{Succ, Unsigned, Zero};

/// Trait for types that can provide a raw mutable pointer to their deref target.
///
/// This is the raw-pointer counterpart of [`Deref`]. It enables traversal of dereference chains
/// without creating intermediate references, which is needed for
/// [`Observer::relocate`](crate::observe::Observer::relocate) to avoid Stacked Borrows retagging.
///
/// # Safety
///
/// The returned pointer from [`deref_ptr`](DerefPtr::deref_ptr) must point to the same location
/// as [`Deref::deref`] would return. Additionally, it must NOT retag the target memory during
/// its construction. It is acceptable to retag `Self` (e.g., by creating `&Self` to read metadata)
/// as long as `Self` and `Target` do not overlap in memory.
pub unsafe trait DerefPtr: Deref {
    /// Returns a raw mutable pointer to the deref target.
    ///
    /// # Safety
    ///
    /// The caller must ensure `this` is a valid pointer to `Self`.
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target;
}

unsafe impl<T: ?Sized> DerefPtr for &T {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { (*this) as *const T as *mut T }
    }
}

unsafe impl<T: ?Sized> DerefPtr for &mut T {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { *this }
    }
}

unsafe impl<T: ?Sized> DerefPtr for Box<T> {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { (&*this).deref() as *const T as *mut T }
    }
}

unsafe impl<T: ?Sized> DerefPtr for Rc<T> {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { Rc::as_ptr(&*this) as *mut T }
    }
}

unsafe impl<T: ?Sized> DerefPtr for Arc<T> {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { Arc::as_ptr(&*this) as *mut T }
    }
}

unsafe impl<'a, B: ToOwned + ?Sized> DerefPtr for Cow<'a, B> {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { (*this).deref() as *const B as *mut B }
    }
}

unsafe impl<T> DerefPtr for Vec<T> {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe {
            let vec = &*this;
            std::ptr::slice_from_raw_parts_mut(vec.as_ptr() as *mut T, vec.len())
        }
    }
}

unsafe impl DerefPtr for String {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe {
            let s = &*this;
            std::mem::transmute::<*mut [u8], *mut str>(std::ptr::slice_from_raw_parts_mut(
                s.as_ptr() as *mut u8,
                s.len(),
            ))
        }
    }
}

unsafe impl DerefPtr for OsString {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { (&*this).deref() as *const _ as *mut _ }
    }
}

unsafe impl DerefPtr for CString {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { (&*this).deref() as *const _ as *mut _ }
    }
}

unsafe impl DerefPtr for PathBuf {
    unsafe fn deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { (&*this).deref() as *const _ as *mut _ }
    }
}

/// Trait for types that can be dereferenced `N` times (inductive version).
///
/// See the [module documentation](self) for details about inductive vs. coinductive.
pub trait AsDeref<N: Unsigned> {
    /// The target type after `N` dereferences.
    type Target: ?Sized;

    /// Dereferences self `N` times.
    fn as_deref(&self) -> &Self::Target;

    /// Returns a raw mutable pointer to the target after `N` dereferences.
    ///
    /// # Safety
    ///
    /// The caller must ensure `this` is a valid pointer to `Self`.
    unsafe fn as_deref_ptr(this: *mut Self) -> *mut Self::Target;
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

    unsafe fn as_deref_ptr(this: *mut Self) -> *mut Self::Target {
        this
    }
}

impl<T: ?Sized> AsDerefMut<Zero> for T {
    fn as_deref_mut(&mut self) -> &mut T {
        self
    }
}

impl<T: AsDeref<N, Target: DerefPtr> + ?Sized, N: Unsigned> AsDeref<Succ<N>> for T {
    type Target = <T::Target as Deref>::Target;

    fn as_deref(&self) -> &Self::Target {
        self.as_deref().deref()
    }

    unsafe fn as_deref_ptr(this: *mut Self) -> *mut Self::Target {
        unsafe { DerefPtr::deref_ptr(AsDeref::<N>::as_deref_ptr(this)) }
    }
}

impl<T: AsDerefMut<N, Target: DerefMut + DerefPtr> + ?Sized, N: Unsigned> AsDerefMut<Succ<N>> for T {
    fn as_deref_mut(&mut self) -> &mut Self::Target {
        self.as_deref_mut().deref_mut()
    }
}

/// Extension trait providing [`AsDeref::as_deref_ptr`] as a method on raw pointers.
#[allow(clippy::wrong_self_convention)]
pub trait AsDerefPtrExt {
    /// The type behind the raw pointer.
    type Pointee: ?Sized;

    /// Dereferences the pointer `D` times, returning a raw mutable pointer to the target.
    ///
    /// # Safety
    ///
    /// The pointer must be valid for the pointee type.
    unsafe fn as_deref_ptr<D>(self) -> *mut <Self::Pointee as AsDeref<D>>::Target
    where
        D: Unsigned,
        Self::Pointee: AsDeref<D>;
}

impl<T: ?Sized> AsDerefPtrExt for *mut T {
    type Pointee = T;

    unsafe fn as_deref_ptr<D>(self) -> *mut <T as AsDeref<D>>::Target
    where
        D: Unsigned,
        Self::Pointee: AsDeref<D>,
    {
        unsafe { AsDeref::<D>::as_deref_ptr(self) }
    }
}

impl<T: ?Sized> AsDerefPtrExt for *const T {
    type Pointee = T;

    unsafe fn as_deref_ptr<D>(self) -> *mut <T as AsDeref<D>>::Target
    where
        D: Unsigned,
        Self::Pointee: AsDeref<D>,
    {
        unsafe { AsDeref::<D>::as_deref_ptr(self.cast_mut()) }
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
