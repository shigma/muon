use std::ffi::CString;

use crate::Observe;
use crate::general::UnsizeObserver;
use crate::helper::{AsDeref, AsDerefMut, Succ, Unsigned};
use crate::impls::DerefObserver;
use crate::observe::{DefaultSpec, RefObserve};

impl Observe for CString {
    type Observer<'ob, S, D>
        = DerefObserver<UnsizeObserver<'ob, S, Succ<D>>>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl RefObserve for CString {
    type Observer<'ob, S, D>
        = DerefObserver<UnsizeObserver<'ob, S, Succ<D>>>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}
