use std::ffi::CStr;

use crate::Observe;
use crate::general::{Unsize, UnsizeObserver};
use crate::helper::{AsDeref, AsDerefMut, Unsigned};
use crate::observe::{DefaultSpec, RefObserve};

impl Unsize for CStr {
    type Slice = [u8];

    fn len(&self) -> usize {
        self.to_bytes().len()
    }

    fn range_from(&self, from: usize) -> &Self::Slice {
        &self.to_bytes()[from..]
    }

    unsafe fn removed_len(_ptr: *const u8, new_len: usize, old_len: usize) -> usize {
        old_len - new_len
    }
}

impl Observe for CStr {
    type Observer<'ob, S, D>
        = UnsizeObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl RefObserve for CStr {
    type Observer<'ob, S, D>
        = UnsizeObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}
