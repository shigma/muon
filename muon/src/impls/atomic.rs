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
                fn flush(&self, snapshot: Self::Snapshot) -> Mutations<'_> {
                    let value = self.to_snapshot();
                    if value != snapshot {
                        Mutations::replace(value)
                    } else {
                        Mutations::new()
                    }
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::{Adapter, Json};
    use crate::general::{SerializeSnapshot, Snapshot};
    use crate::observe::{ObserveExt, SerializeObserverExt};

    const VALUE: u64 = 0xDEAD_BEEF_1234_5678;

    #[test]
    fn snapshot_flush_emits_loaded_value() {
        let atomic = AtomicU64::new(0);
        let snapshot = atomic.to_snapshot();
        assert!(atomic.flush(snapshot).is_empty());
        atomic.store(VALUE, Ordering::Relaxed);
        let mutations = atomic.flush(snapshot);
        // The emitted value is the one loaded during flush, not a later one.
        atomic.store(1, Ordering::Relaxed);
        let Json(mutation) = Json::from_mutations(mutations).unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(VALUE))));
    }

    #[test]
    fn observer_flush() {
        let mut atomic = AtomicU64::new(0);
        let mut ob = atomic.__observe();
        ob.store(VALUE, Ordering::Relaxed);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(VALUE))));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }
}
