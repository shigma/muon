use std::sync::atomic::Ordering;

use crate::general::{Snapshot, SnapshotObserver};
use crate::helper::{AsDeref, AsDerefMut, Unsigned};
use crate::observe::{DefaultSpec, Observe, RefObserve};

macro_rules! impl_atomic {
    ($($ident:ident => $output:ty),* $(,)?) => {
        $(
            impl Snapshot for std::sync::atomic::$ident {
                type Snapshot = $output;

                fn to_snapshot(&self) -> Self::Snapshot {
                    self.load(Ordering::Relaxed)
                }

                fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
                    self.load(Ordering::Relaxed) == *snapshot
                }
            }

            impl Observe for std::sync::atomic::$ident {
                type Observer<'ob, S, D>
                    = SnapshotObserver<'ob, S, D>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

                type Spec = DefaultSpec;
            }

            impl RefObserve for std::sync::atomic::$ident {
                type Observer<'ob, S, D>
                    = SnapshotObserver<'ob, S, D>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDeref<D, Target = Self> + ?Sized + 'ob;

                type Spec = DefaultSpec;
            }
        )*
    };
}

impl_atomic! {
    AtomicBool => bool,
    AtomicU8 => u8,
    AtomicU16 => u16,
    AtomicU32 => u32,
    AtomicU64 => u64,
    AtomicUsize => usize,
    AtomicI8 => i8,
    AtomicI16 => i16,
    AtomicI32 => i32,
    AtomicI64 => i64,
    AtomicIsize => isize,
}
