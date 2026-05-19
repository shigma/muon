//! Observer implementation for [`Path`].

use std::ffi::OsStr;
use std::marker::PhantomData;
use std::path::Path;
use std::ptr::NonNull;

use crate::Mutations;
use crate::helper::shallow::{ShallowDelegate, ObserverState, SerializeObserverState, shallow_observer};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, Unsigned, Zero};
use crate::impls::strings::TruncateLen;
use crate::impls::strings::os_str::OsStrObserver;
use crate::observe::{DefaultSpec, Observe, RefObserve};

shallow_observer! {
    /// Observer implementation for [`Path`].
    struct PathObserver<V>(pub(crate) Path, pub(crate) V);
}

impl<'ob, V, S: ?Sized, D> PathObserver<'ob, V, S, D>
where
    V: Invalidate<()> + Invalidate<Path> + Invalidate<OsStr>,
    D: Unsigned,
    S: AsDerefMut<D, Target = Path>,
{
    /// See [`Path::as_mut_os_str`].
    pub fn as_mut_os_str(&mut self) -> OsStrObserver<'_, ShallowDelegate<V>, OsStr> {
        let state = ShallowDelegate::new(&raw mut self.state);
        let os_str = (*self.ptr).as_deref_mut().as_mut_os_str();
        let ob = OsStrObserver {
            state,
            ptr: Pointer::new(os_str),
            phantom: PhantomData,
        };
        Pointer::register_state::<_, Zero>(&ob.ptr, &ob.state);
        ob
    }
}

pub struct PathRefObserverState {
    raw_parts: Option<Option<(NonNull<()>, usize)>>,
}

impl Invalidate<Path> for PathRefObserverState {
    fn invalidate(&mut self, value: &Path) {
        self.raw_parts.get_or_insert_with(|| {
            value
                .to_str()
                .map(|str| (NonNull::from(str).cast::<()>(), str.truncate_len()))
        });
    }
}

impl ObserverState<Path> for PathRefObserverState {
    fn observe(_: &Path) -> Self {
        Self { raw_parts: None }
    }
}

impl SerializeObserverState<Path> for PathRefObserverState {
    fn flush(&mut self, value: &Path) -> Mutations {
        let (old_addr, old_len) = match self.raw_parts.take() {
            None => return Mutations::new(),
            Some(None) => return Mutations::replace(value),
            Some(Some(parts)) => parts,
        };
        let Some(str) = value.to_str() else {
            return Mutations::replace(value);
        };
        let new_addr = NonNull::from(str).cast::<()>();
        let new_len = str.truncate_len();
        if new_addr != old_addr {
            return Mutations::replace(value);
        }
        if new_len < old_len {
            #[cfg(not(feature = "truncate"))]
            return Mutations::replace(value);
            #[cfg(feature = "truncate")]
            return Mutations::truncate(old_len - new_len);
        }
        if new_len > old_len {
            #[cfg(not(feature = "append"))]
            return Mutations::replace(value);
            #[cfg(feature = "append")]
            return Mutations::append(&str[old_len..]);
        }
        Mutations::new()
    }
}

impl Observe for Path {
    type Observer<'ob, S, D>
        = PathObserver<'ob, bool, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

impl RefObserve for Path {
    type Observer<'ob, S, D>
        = PathObserver<'ob, PathRefObserverState, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDeref<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}
