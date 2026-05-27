use std::sync::atomic::Ordering;

use crate::general::{SerializeSnapshot, Snapshot, SnapshotObserver};
use crate::helper::{AsDeref, AsDerefMut, Unsigned};
use crate::observe::{DefaultSpec, RoObserve};
use crate::{Mutations, Observe};

macro_rules! impl_atomic {
    ($($ident:ident => $output:ty),* $(,)?) => {
        $(
            impl Snapshot for std::sync::atomic::$ident {
                type Snapshot = $output;

                fn to_snapshot(&self) -> Self::Snapshot {
                    self.load(Ordering::Relaxed)
                }
            }

            impl SerializeSnapshot for std::sync::atomic::$ident {
                fn flush(&self, snapshot: Self::Snapshot) -> Mutations {
                    self.to_snapshot().flush(snapshot)
                }
            }

            impl Observe for std::sync::atomic::$ident {
                type Observer<'ob, S, D>
                    = SnapshotObserver<'ob, Self, S, D>
                where
                    Self: 'ob,
                    D: Unsigned,
                    S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

                type Spec = DefaultSpec;
            }

            impl RoObserve for std::sync::atomic::$ident {
                type Observer<'ob, S, D>
                    = SnapshotObserver<'ob, Self, S, D>
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
