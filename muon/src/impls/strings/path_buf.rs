//! Observer implementation for [`PathBuf`].

use std::collections::TryReserveError;
use std::ffi::{OsStr, OsString};
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use super::TruncateLen;
use super::os_str::OsStrObserver;
use super::os_string::OsStringObserver;
use super::path::PathObserver;
use crate::general::{SerializeSnapshot, Snapshot};
use crate::helper::macros::{default_impl_ro_observe, delegate_methods};
use crate::helper::shallow::{ObserverState, SerializeObserverState, ShallowDelegate};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, Pointer, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{DefaultSpec, Observer, SerializeObserver};
use crate::{MutationKind, Mutations, Observe};

pub struct PathBufObserverState {
    pub append_index: usize,
    pub truncate_len: usize,
}

impl PathBufObserverState {
    fn mark_truncate(&mut self, preserved_bytes: usize, path_bytes: &[u8]) {
        if self.append_index <= preserved_bytes {
            return;
        }
        let removed = &path_bytes[preserved_bytes..self.append_index];
        match std::str::from_utf8(removed) {
            Ok(s) => {
                self.truncate_len += s.truncate_len();
                self.append_index = preserved_bytes;
            }
            Err(_) => {
                self.append_index = 0;
                self.truncate_len = self.truncate_len.max(1);
            }
        }
    }
}

impl<T: ?Sized> Invalidate<T> for PathBufObserverState {
    fn invalidate(&mut self, _value: &T) {
        if self.append_index > 0 {
            self.append_index = 0;
            self.truncate_len = self.truncate_len.max(1);
        }
    }
}

impl ObserverState<Path> for PathBufObserverState {
    fn observe(value: &Path) -> Self {
        Self {
            append_index: value.as_os_str().len(),
            truncate_len: 0,
        }
    }
}

impl SerializeObserverState<Path> for PathBufObserverState {
    fn flush(&mut self, value: &Path) -> Mutations {
        let truncate_len = std::mem::replace(&mut self.truncate_len, 0);
        let Some(str) = value.to_str() else {
            self.append_index = 0;
            return Mutations::replace(value as &Path);
        };
        let old_append_index = std::mem::replace(&mut self.append_index, str.len());
        if old_append_index == 0 && truncate_len > 0 {
            return Mutations::replace(value as &Path);
        }
        let mut mutations = Mutations::new();
        if truncate_len > 0 {
            #[cfg(feature = "truncate")]
            mutations.extend(MutationKind::Truncate(truncate_len));
            #[cfg(not(feature = "truncate"))]
            return Mutations::replace(value as &Path);
        }
        if str.len() > old_append_index {
            #[cfg(feature = "append")]
            mutations.extend(Mutations::append(&str[old_append_index..]));
            #[cfg(not(feature = "append"))]
            return Mutations::replace(value as &Path);
        }
        mutations
    }
}

/// Observer implementation for [`PathBuf`].
pub struct PathBufObserver<'ob, S: ?Sized, D = Zero> {
    inner: PathObserver<'ob, PathBufObserverState, S, Succ<D>>,
}

impl<'ob, S: ?Sized, D> Deref for PathBufObserver<'ob, S, D> {
    type Target = PathObserver<'ob, PathBufObserverState, S, Succ<D>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'ob, S: ?Sized, D> DerefMut for PathBufObserver<'ob, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'ob, S: ?Sized, D> QuasiObserver for PathBufObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = PathBuf>,
{
    type Head = S;
    type OuterDepth = Succ<Succ<Zero>>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        if this.inner.state.append_index > 0 {
            this.inner.state.append_index = 0;
            this.inner.state.truncate_len = this.inner.state.truncate_len.max(1);
        }
    }
}

impl<'ob, S: ?Sized, D> Observer for PathBufObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = PathBuf>,
{
    unsafe fn observe(head: *mut Self::Head) -> Self {
        Self {
            inner: unsafe { Observer::observe(head) },
        }
    }

    unsafe fn relocate(this: &mut Self, head: *mut Self::Head) {
        unsafe { Observer::relocate(&mut this.inner, head) }
    }
}

impl<'ob, S: ?Sized, D> SerializeObserver for PathBufObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = PathBuf>,
{
    fn flush(this: &mut Self) -> Mutations {
        SerializeObserver::flush(&mut this.inner)
    }
}

impl<'ob, S: ?Sized, D> PathBufObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = PathBuf>,
{
    /// See [`PathBuf::push`].
    pub fn push<P: AsRef<Path>>(&mut self, path: P) {
        let path_ref = path.as_ref();
        if path_ref.has_root() || path_ref.is_absolute() {
            let value = (*self.inner.ptr).as_deref_mut();
            self.inner.state.mark_truncate(0, value.as_os_str().as_encoded_bytes());
            value.push(path_ref);
        } else {
            (*self.inner.ptr).as_deref_mut().push(path_ref);
        }
    }

    /// See [`PathBuf::pop`].
    pub fn pop(&mut self) -> bool {
        let value = (*self.inner.ptr).as_deref_mut();
        let preserved = value.parent().map_or(value.as_os_str().len(), |p| p.as_os_str().len());
        self.inner
            .state
            .mark_truncate(preserved, value.as_os_str().as_encoded_bytes());
        value.pop()
    }

    /// See [`PathBuf::set_file_name`].
    pub fn set_file_name<S2: AsRef<OsStr>>(&mut self, file_name: S2) {
        let value = (*self.inner.ptr).as_deref_mut();
        let preserved = value.parent().map_or(value.as_os_str().len(), |p| p.as_os_str().len());
        self.inner
            .state
            .mark_truncate(preserved, value.as_os_str().as_encoded_bytes());
        value.set_file_name(file_name);
    }

    /// See [`PathBuf::set_extension`].
    pub fn set_extension<S2: AsRef<OsStr>>(&mut self, extension: S2) -> bool {
        let value = (*self.inner.ptr).as_deref_mut();
        let preserved = value.as_os_str().len() - value.extension().map_or(0, |e| e.len() + 1);
        self.inner
            .state
            .mark_truncate(preserved, value.as_os_str().as_encoded_bytes());
        value.set_extension(extension)
    }

    /// See [`PathBuf::add_extension`].
    pub fn add_extension<S2: AsRef<OsStr>>(&mut self, extension: S2) -> bool {
        (*self.inner.ptr).as_deref_mut().add_extension(extension)
    }

    /// See [`PathBuf::as_mut_os_string`].
    pub fn as_mut_os_string(&mut self) -> OsStringObserver<'_, ShallowDelegate<PathBufObserverState>, OsString> {
        let state = ShallowDelegate::new(&raw mut self.inner.state);
        let os_string = (*self.inner.ptr).as_deref_mut().as_mut_os_string();
        let inner_ob = OsStrObserver {
            state,
            ptr: Pointer::new(os_string),
            phantom: PhantomData,
        };
        Pointer::register_state::<_, Succ<Zero>>(&inner_ob.ptr, &inner_ob.state);
        OsStringObserver { inner: inner_ob }
    }

    /// See [`PathBuf::clear`].
    pub fn clear(&mut self) {
        let value = (*self.inner.ptr).as_deref_mut();
        self.inner.state.mark_truncate(0, value.as_os_str().as_encoded_bytes());
        value.clear();
    }

    delegate_methods! { untracked_mut() as PathBuf =>
        pub fn reserve(&mut self, additional: usize);
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn reserve_exact(&mut self, additional: usize);
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
    }
}

impl<'ob, S: ?Sized, D, P: AsRef<Path>> Extend<P> for PathBufObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = PathBuf>,
{
    fn extend<I: IntoIterator<Item = P>>(&mut self, iter: I) {
        for item in iter {
            self.push(item);
        }
    }
}

impl<'ob, S: ?Sized, D> Debug for PathBufObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = PathBuf>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PathBufObserver").field(&self.untracked_ref()).finish()
    }
}

impl<'ob, S: ?Sized, D> Display for PathBufObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = PathBuf>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.untracked_ref().display(), f)
    }
}

impl Observe for PathBuf {
    type Observer<'ob, S, D>
        = PathBufObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ro_observe! {
    impl RoObserve for PathBuf;
}

impl Snapshot for PathBuf {
    type Snapshot = Option<Box<str>>;

    fn to_snapshot(&self) -> Option<Box<str>> {
        self.as_path().to_snapshot()
    }
}

impl SerializeSnapshot for PathBuf {
    fn flush(&self, snapshot: Option<Box<str>>) -> Mutations {
        self.as_path().flush(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_mutation_returns_none() {
        let mut p = PathBuf::from("/usr/local");
        let mut ob = p.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_on_deref_mut() {
        let mut p = PathBuf::from("/usr/local");
        let mut ob = p.__observe();
        *ob.tracked_mut() = PathBuf::from("/etc");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("/etc"))));
    }

    #[test]
    fn push_relative_as_append() {
        let mut p = PathBuf::from("/usr");
        let mut ob = p.__observe();
        ob.push("local");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("/local"))));
    }

    #[test]
    fn push_absolute_as_replace() {
        let mut p = PathBuf::from("/usr");
        let mut ob = p.__observe();
        ob.push("/etc");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("/etc"))));
    }

    #[test]
    fn pop_as_truncate() {
        let mut p = PathBuf::from("/usr/local/bin");
        let mut ob = p.__observe();
        ob.pop();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 4)));
    }

    #[test]
    fn pop_then_push_relative() {
        let mut p = PathBuf::from("/usr/local");
        let mut ob = p.__observe();
        ob.pop();
        ob.push("etc");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 6), append!(_, json!("/etc")))));
    }

    #[test]
    fn set_file_name_as_truncate_append() {
        let mut p = PathBuf::from("/usr/file.txt");
        let mut ob = p.__observe();
        ob.set_file_name("other.rs");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 9), append!(_, json!("/other.rs"))))
        );
    }

    #[test]
    fn set_extension_as_truncate_append() {
        let mut p = PathBuf::from("/usr/file.txt");
        let mut ob = p.__observe();
        ob.set_extension("rs");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!(".rs")))));
    }

    #[test]
    fn set_extension_no_existing() {
        let mut p = PathBuf::from("/usr/file");
        let mut ob = p.__observe();
        ob.set_extension("rs");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(".rs"))));
    }

    #[test]
    fn add_extension_as_append() {
        let mut p = PathBuf::from("/usr/file.tar");
        let mut ob = p.__observe();
        ob.add_extension("gz");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!(".gz"))));
    }

    #[test]
    fn clear_then_push() {
        let mut p = PathBuf::from("/usr");
        let mut ob = p.__observe();
        ob.clear();
        ob.push("/etc");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("/etc"))));
    }

    #[test]
    fn capacity_only_no_mutation() {
        let mut p = PathBuf::from("/usr/local");
        let mut ob = p.__observe();
        ob.reserve(100);
        ob.shrink_to_fit();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn as_mut_os_string_tracked() {
        let mut p = PathBuf::from("/usr/local");
        let mut ob = p.__observe();
        {
            let mut os_ob = ob.as_mut_os_string();
            os_ob.tracked_mut().push("/bin");
        }
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("/usr/local/bin"))));
    }

    #[test]
    fn as_mut_os_str_make_uppercase() {
        let mut p = PathBuf::from("/usr/local");
        let mut ob = p.__observe();
        {
            let mut os_ob = ob.as_mut_os_str();
            os_ob.make_ascii_uppercase();
        }
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("/USR/LOCAL"))));
    }

    #[test]
    fn as_mut_os_str_empty_no_mutation() {
        let mut p = PathBuf::new();
        let mut ob = p.__observe();
        {
            let mut os_ob = ob.as_mut_os_str();
            os_ob.make_ascii_uppercase();
        }
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn clear_empty_no_mutation() {
        let mut p = PathBuf::new();
        let mut ob = p.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn pop_multi_byte() {
        let mut p = PathBuf::from("/usr/日本語");
        let mut ob = p.__observe();
        ob.pop();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 4)));
    }

    #[test]
    fn set_extension_remove() {
        let mut p = PathBuf::from("/usr/file.txt");
        let mut ob = p.__observe();
        ob.set_extension("");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 4)));
    }

    #[test]
    fn multiple_pops() {
        let mut p = PathBuf::from("/a/b/c/d");
        let mut ob = p.__observe();
        ob.pop();
        ob.pop();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 4)));
    }
}
