//! Observer implementation for [`OsStr`].

use std::ffi::OsStr;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::ptr::NonNull;

use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::delegate_methods;
use crate::helper::shallow::{ObserverState, SerializeObserverState, shallow_observer};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, QuasiObserver, Unsigned};
use crate::observe::{DefaultSpec, RoObserve};
use crate::{Mutations, Observe};

#[cfg(unix)]
pub(super) fn os_str_len(value: &OsStr) -> usize {
    value.as_bytes().len()
}

#[cfg(windows)]
pub(super) fn os_str_len(value: &OsStr) -> usize {
    value.encode_wide().count()
}

shallow_observer! {
    /// Observer implementation for [`OsStr`].
    struct OsStrObserver<V>(pub(crate) OsStr, pub(crate) V);
}

impl<'ob, V, S: ?Sized, D> OsStrObserver<'ob, V, S, D>
where
    V: Invalidate<OsStr>,
    D: Unsigned,
    S: AsDerefMut<D, Target = OsStr>,
{
    fn nonempty_mut(&mut self) -> &mut OsStr {
        if (*self).untracked_ref().is_empty() {
            self.untracked_mut()
        } else {
            self.tracked_mut()
        }
    }

    delegate_methods! { nonempty_mut() as OsStr =>
        pub fn make_ascii_uppercase(&mut self);
        pub fn make_ascii_lowercase(&mut self);
    }
}

pub struct OsStrRoObserverState {
    raw_parts: Option<(NonNull<()>, usize)>,
}

impl Invalidate<OsStr> for OsStrRoObserverState {
    fn invalidate(&mut self, value: &OsStr) {
        self.raw_parts
            .get_or_insert_with(|| (NonNull::from(value).cast::<()>(), os_str_len(value)));
    }
}

impl ObserverState<OsStr> for OsStrRoObserverState {
    fn observe(_: &OsStr) -> Self {
        Self { raw_parts: None }
    }
}

impl SerializeObserverState<OsStr> for OsStrRoObserverState {
    fn flush<'a>(&'a mut self, value: &'a OsStr) -> Mutations<'a> {
        let Some((old_addr, old_len)) = self.raw_parts.take() else {
            return Mutations::new();
        };
        let new_addr = NonNull::from(value).cast::<()>();
        let new_len = os_str_len(value);
        if new_addr != old_addr {
            return Mutations::replace(value);
        }
        if new_len < old_len {
            #[cfg(not(feature = "truncate"))]
            return Mutations::replace(value);
            #[cfg(feature = "truncate")]
            {
                #[cfg(unix)]
                return Mutations::truncate(old_len - new_len).with_prefix("Unix");
                #[cfg(windows)]
                return Mutations::truncate(old_len - new_len).with_prefix("Windows");
            }
        }
        if new_len > old_len {
            #[cfg(not(feature = "append"))]
            return Mutations::replace(value);
            #[cfg(feature = "append")]
            {
                #[cfg(unix)]
                return Mutations::append(&value.as_bytes()[old_len..]).with_prefix("Unix");
                #[cfg(windows)]
                return Mutations::append(value.encode_wide().skip(old_len).collect::<Vec<_>>()).with_prefix("Windows");
            }
        }
        Mutations::new()
    }
}

impl Observe for OsStr {
    type Observer<'ob, S, D>
        = OsStrObserver<'ob, bool, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl RoObserve for OsStr {
    type Observer<'ob, S, D>
        = OsStrObserver<'ob, OsStrRoObserverState, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl Snapshot for OsStr {
    #[cfg(unix)]
    type Snapshot = Box<[u8]>;
    #[cfg(windows)]
    type Snapshot = Box<[u16]>;

    fn to_snapshot(&self) -> Self::Snapshot {
        #[cfg(unix)]
        return self.as_bytes().into();
        #[cfg(windows)]
        return self.encode_wide().collect();
    }
}

impl SerializeSnapshot for OsStr {
    fn flush(&self, snapshot: Self::Snapshot) -> Mutations<'_> {
        #[cfg(unix)]
        return self.as_bytes().flush(snapshot).with_prefix("Unix");
        #[cfg(windows)]
        {
            // `encode_wide` yields a temporary buffer, so emit it by ownership as a whole replace.
            let wide = self.to_snapshot();
            if wide == snapshot {
                return Mutations::new();
            }
            return Mutations::replace(wide).with_prefix("Windows");
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsStr;

    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::{Adapter, Json};
    use crate::general::{SerializeSnapshot, Snapshot};

    #[test]
    fn snapshot_flush_no_change() {
        let value = OsStr::new("hello");
        assert!(value.flush(value.to_snapshot()).is_empty());
    }

    #[test]
    fn snapshot_flush_append() {
        let value = OsStr::new("hello world");
        let snapshot = OsStr::new("hello").to_snapshot();
        let Json(mutation) = Json::from_mutations(value.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(append!(Unix, json!(b" world"))));
    }

    #[test]
    fn snapshot_flush_element_change() {
        let value = OsStr::new("hallo");
        let snapshot = OsStr::new("hello").to_snapshot();
        let Json(mutation) = Json::from_mutations(value.flush(snapshot)).unwrap();
        assert_eq!(mutation, Some(replace!(Unix.-4, json!(b'a'))));
    }
}
