//! Observer implementation for [`OsString`].

use std::collections::TryReserveError;
use std::ffi::{OsStr, OsString};
use std::fmt::{Debug, Display, Write};
use std::ops::{Deref, DerefMut, Index, IndexMut, RangeFull};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

use super::os_str::{OsStrObserver, os_str_len};
use crate::helper::macros::{default_impl_ref_observe, delegate_methods};
use crate::helper::shallow::{ObserverState, SerializeObserverState};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, QuasiObserver, Succ, Unsigned, Zero};
use crate::observe::{DefaultSpec, Observer, SerializeObserver};
use crate::{MutationKind, Mutations, Observe};

pub struct OsStringObserverState {
    pub append_index: usize,
    pub truncate_len: usize,
}

impl OsStringObserverState {
    fn mark_truncate(&mut self, new_len: usize) {
        if self.append_index <= new_len {
            return;
        }
        self.truncate_len += self.append_index - new_len;
        self.append_index = new_len;
    }
}

impl Invalidate<OsStr> for OsStringObserverState {
    fn invalidate(&mut self, _value: &OsStr) {
        self.mark_truncate(0);
    }
}

impl Invalidate<()> for OsStringObserverState {
    fn invalidate(&mut self, _: &()) {
        self.append_index = 0;
        self.truncate_len = self.truncate_len.max(1);
    }
}

impl ObserverState<OsStr> for OsStringObserverState {
    fn observe(value: &OsStr) -> Self {
        Self {
            append_index: os_str_len(value),
            truncate_len: 0,
        }
    }
}

impl SerializeObserverState<OsStr> for OsStringObserverState {
    fn flush(&mut self, value: &OsStr) -> Mutations {
        let new_len = os_str_len(value);
        let append_index = std::mem::replace(&mut self.append_index, new_len);
        let truncate_len = std::mem::replace(&mut self.truncate_len, 0);
        if append_index == 0 && truncate_len > 0 {
            return Mutations::replace(value as &OsStr);
        }
        let mut mutations = Mutations::new();
        if truncate_len > 0 {
            #[cfg(feature = "truncate")]
            mutations.extend(MutationKind::Truncate(truncate_len));
            #[cfg(not(feature = "truncate"))]
            return Mutations::replace(value as &OsStr);
        }
        if new_len > append_index {
            #[cfg(feature = "append")]
            {
                #[cfg(unix)]
                mutations.extend(Mutations::append(&value.as_bytes()[append_index..]));
                #[cfg(windows)]
                mutations.extend(Mutations::append_owned(
                    value.encode_wide().skip(append_index).collect::<Vec<_>>(),
                ));
            }
            #[cfg(not(feature = "append"))]
            return Mutations::replace(value as &OsStr);
        }
        #[cfg(unix)]
        return mutations.with_prefix("Unix");
        #[cfg(windows)]
        return mutations.with_prefix("Windows");
    }
}

/// Observer implementation for [`OsString`].
pub struct OsStringObserver<'ob, V, S: ?Sized, D = Zero> {
    pub(super) inner: OsStrObserver<'ob, V, S, Succ<D>>,
}

impl<'ob, V, S: ?Sized, D> Deref for OsStringObserver<'ob, V, S, D> {
    type Target = OsStrObserver<'ob, V, S, Succ<D>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'ob, V, S: ?Sized, D> DerefMut for OsStringObserver<'ob, V, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'ob, V, S: ?Sized, D> QuasiObserver for OsStringObserver<'ob, V, S, D>
where
    V: Invalidate<OsStr>,
    D: Unsigned,
    S: AsDeref<D, Target = OsString>,
{
    type Head = S;
    type OuterDepth = Succ<Succ<Zero>>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        Invalidate::invalidate(&mut this.inner.state, (*this.inner.ptr).as_deref().as_os_str());
    }
}

impl<'ob, S: ?Sized, D> Observer for OsStringObserver<'ob, OsStringObserverState, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
{
    fn observe(head: &mut Self::Head) -> Self {
        Self {
            inner: Observer::observe(head),
        }
    }

    unsafe fn relocate(this: &mut Self, head: &mut Self::Head) {
        unsafe { Observer::relocate(&mut this.inner, head) }
    }
}

impl<'ob, S: ?Sized, D> SerializeObserver for OsStringObserver<'ob, OsStringObserverState, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = OsString>,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        unsafe { SerializeObserver::flush(&mut this.inner) }
    }
}

// Methods requiring OsStringObserverState (append/truncate tracking)
impl<'ob, S: ?Sized, D> OsStringObserver<'ob, OsStringObserverState, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
{
    /// See [`OsString::push`].
    pub fn push<T: AsRef<OsStr>>(&mut self, s: T) {
        self.untracked_mut().push(s);
    }

    /// See [`OsString::clear`].
    pub fn clear(&mut self) {
        let state = &mut self.inner.state;
        state.mark_truncate(0);
        (*self.inner.ptr).as_deref_mut().clear();
    }
}

// Capacity-only methods (generic over V)
impl<'ob, V, S: ?Sized, D> OsStringObserver<'ob, V, S, D>
where
    V: Invalidate<OsStr>,
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
{
    delegate_methods! { untracked_mut() as OsString =>
        pub fn reserve(&mut self, additional: usize);
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn reserve_exact(&mut self, additional: usize);
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
    }
}

impl<'ob, S: ?Sized, D, U> Extend<U> for OsStringObserver<'ob, OsStringObserverState, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
    OsString: Extend<U>,
{
    fn extend<I: IntoIterator<Item = U>>(&mut self, iter: I) {
        self.untracked_mut().extend(iter);
    }
}

impl<'ob, V, S: ?Sized, D> IndexMut<RangeFull> for OsStringObserver<'ob, V, S, D>
where
    V: Invalidate<OsStr>,
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
{
    fn index_mut(&mut self, index: RangeFull) -> &mut Self::Output {
        self.tracked_mut().index_mut(index)
    }
}

impl<'ob, V, S: ?Sized, D> Index<RangeFull> for OsStringObserver<'ob, V, S, D>
where
    V: Invalidate<OsStr>,
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
{
    type Output = OsStr;

    fn index(&self, index: RangeFull) -> &Self::Output {
        self.untracked_ref().index(index)
    }
}

impl<'ob, S: ?Sized, D> Write for OsStringObserver<'ob, OsStringObserverState, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
{
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.untracked_mut().write_str(s)
    }

    fn write_char(&mut self, c: char) -> std::fmt::Result {
        self.untracked_mut().write_char(c)
    }
}

impl<'ob, V, S: ?Sized, D> Debug for OsStringObserver<'ob, V, S, D>
where
    V: Invalidate<OsStr>,
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OsStringObserver").field(&self.untracked_ref()).finish()
    }
}

impl<'ob, V, S: ?Sized, D> Display for OsStringObserver<'ob, V, S, D>
where
    V: Invalidate<OsStr>,
    D: Unsigned,
    S: AsDerefMut<D, Target = OsString>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.untracked_ref().to_string_lossy(), f)
    }
}

impl Observe for OsString {
    type Observer<'ob, S, D>
        = OsStringObserver<'ob, OsStringObserverState, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ref_observe! {
    impl RefObserve for OsString;
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_mutation_returns_none() {
        let mut s = OsString::from("hello");
        let mut ob = s.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_on_deref_mut() {
        let mut s = OsString::from("hello");
        let mut ob = s.__observe();
        *ob.tracked_mut() = OsString::from("world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!({"Unix": [119, 111, 114, 108, 100]}))));
    }

    #[test]
    fn append_with_push() {
        let mut s = OsString::from("foo");
        let mut ob = s.__observe();
        ob.push("bar");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(Unix, json!([98, 97, 114]))));
    }

    #[test]
    fn append_empty_string() {
        let mut s = OsString::from("foo");
        let mut ob = s.__observe();
        ob.push("");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_after_append() {
        let mut s = OsString::from("abc");
        let mut ob = s.__observe();
        ob.push("def");
        *ob.tracked_mut() = OsString::from("xyz");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!({"Unix": [120, 121, 122]}))));
    }

    #[test]
    fn write_str_appends() {
        use std::fmt::Write;
        let mut s = OsString::from("foo");
        let mut ob = s.__observe();
        write!(ob, "bar{}", 42).unwrap();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(Unix, json!([98, 97, 114, 52, 50]))));
    }

    #[test]
    fn extend_appends() {
        let mut s = OsString::from("foo");
        let mut ob = s.__observe();
        ob.extend([OsString::from("bar"), OsString::from("baz")]);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(Unix, json!([98, 97, 114, 98, 97, 122]))));
    }

    #[test]
    fn capacity_only_no_mutation() {
        let mut s = OsString::from("hello");
        let mut ob = s.__observe();
        ob.reserve(100);
        ob.shrink_to_fit();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn clear_empty_no_mutation() {
        let mut s = OsString::new();
        let mut ob = s.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn clear_as_replace() {
        let mut s = OsString::from("hello");
        let mut ob = s.__observe();
        ob.clear();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!({"Unix": []}))));
    }

    #[test]
    fn clear_then_push_as_replace() {
        let mut s = OsString::from("hello");
        let mut ob = s.__observe();
        ob.clear();
        ob.push("world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!({"Unix": [119, 111, 114, 108, 100]}))));
    }

    #[test]
    fn append_after_clear() {
        let mut s = OsString::from("hi");
        let mut ob = s.__observe();
        ob.clear();
        ob.push("hello world");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(replace!(
                _,
                json!({"Unix": [104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100]})
            ))
        );
    }
}
