use std::ffi::CString;

use crate::Mutations;
use crate::general::{SerializeSnapshot, Snapshot, UnsizeObserver};
use crate::helper::{AsDeref, AsDerefMut, Succ, Unsigned};
use crate::impls::DerefObserver;
use crate::observe::{DefaultSpec, Observe, RoObserve};

impl Observe for CString {
    type Observer<'ob, S, D>
        = DerefObserver<UnsizeObserver<'ob, S, Succ<D>>>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl RoObserve for CString {
    type Observer<'ob, S, D>
        = DerefObserver<UnsizeObserver<'ob, S, Succ<D>>>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl Snapshot for CString {
    type Snapshot = Box<[u8]>;

    fn to_snapshot(&self) -> Box<[u8]> {
        self.as_c_str().to_snapshot()
    }
}

impl SerializeSnapshot for CString {
    fn flush(&self, snapshot: Box<[u8]>) -> Mutations {
        self.as_c_str().flush(snapshot)
    }
}
