use std::ffi::CStr;

use crate::Mutations;
use crate::general::{SerializeSnapshot, Snapshot, Unsize, UnsizeObserver};
use crate::helper::{AsDeref, AsDerefMut, Unsigned};
use crate::observe::{DefaultSpec, Observe, RoObserve};

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

impl RoObserve for CStr {
    type Observer<'ob, S, D>
        = UnsizeObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl Snapshot for CStr {
    type Snapshot = Box<[u8]>;

    fn to_snapshot(&self) -> Box<[u8]> {
        self.to_bytes().into()
    }
}

impl SerializeSnapshot for CStr {
    fn flush(&self, snapshot: Box<[u8]>) -> Mutations {
        self.to_bytes().flush(snapshot)
    }
}
