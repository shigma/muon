//! Observer implementation for [`String`].

use std::borrow::Cow;
use std::collections::TryReserveError;
use std::fmt::{Debug, Display, Write};
use std::ops::{AddAssign, Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};
use std::slice::SliceIndex;
use std::string::Drain;

use crate::helper::macros::{default_impl_ref_observe, delegate_methods};
use crate::helper::shallow::{ObserverState, SerializeObserverState, ShallowMut};
use crate::helper::{AsDeref, AsDerefMut, Invalidate, QuasiObserver, Succ, Unsigned, Zero};
use crate::impls::strings::TruncateLen;
use crate::impls::strings::str::StrObserver;
use crate::observe::{DefaultSpec, Observer, SerializeObserver};
use crate::{MutationKind, Mutations, Observe};

pub struct StringObserverState {
    pub append_index: usize, // byte index
    pub truncate_len: usize, // feature-gated: byte/char/utf16 count
}

impl StringObserverState {
    pub fn mark_truncate(&mut self, value: &str, index: usize) {
        if self.append_index <= index {
            return;
        }
        self.truncate_len += value[index..self.append_index].truncate_len();
        self.append_index = index;
    }
}

impl<T: ?Sized> Invalidate<T> for StringObserverState {
    fn invalidate(&mut self, _: &T) {
        if self.append_index > 0 {
            self.append_index = 0;
            self.truncate_len = self.truncate_len.max(1);
        }
    }
}

impl<T: AsRef<str> + ?Sized> ObserverState<T> for StringObserverState {
    fn observe(value: &T) -> Self {
        Self {
            append_index: value.as_ref().len(),
            truncate_len: 0,
        }
    }
}

impl<T: AsRef<str> + ?Sized> SerializeObserverState<T> for StringObserverState {
    fn flush(&mut self, value: &T) -> Mutations {
        let value = value.as_ref();
        let len = value.len();
        let append_index = std::mem::replace(&mut self.append_index, len);
        let truncate_len = std::mem::replace(&mut self.truncate_len, 0);
        if append_index == 0 && truncate_len > 0 {
            return Mutations::replace(value);
        }
        let mut mutations = Mutations::new();
        if truncate_len > 0 {
            #[cfg(feature = "truncate")]
            mutations.extend(MutationKind::Truncate(truncate_len));
            #[cfg(not(feature = "truncate"))]
            return Mutations::replace(value);
        }
        if len > append_index {
            #[cfg(feature = "append")]
            mutations.extend(Mutations::append(&value[append_index..]));
            #[cfg(not(feature = "append"))]
            return Mutations::replace(value);
        }
        mutations
    }
}

/// Observer implementation for [`String`].
pub struct StringObserver<'ob, S: ?Sized, D = Zero> {
    inner: StrObserver<'ob, StringObserverState, S, Succ<D>>,
}

impl<'ob, S: ?Sized, D> Deref for StringObserver<'ob, S, D> {
    type Target = StrObserver<'ob, StringObserverState, S, Succ<D>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'ob, S: ?Sized, D> DerefMut for StringObserver<'ob, S, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'ob, S: ?Sized, D> QuasiObserver for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = String>,
{
    type Head = S;
    type OuterDepth = Succ<Succ<Zero>>;
    type InnerDepth = D;

    fn invalidate(this: &mut Self) {
        Invalidate::invalidate(&mut this.inner.state, (*this.inner.ptr).as_deref().as_str());
    }
}

impl<'ob, S: ?Sized, D> Observer for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = String>,
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

impl<'ob, S: ?Sized, D> SerializeObserver for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = String>,
{
    unsafe fn flush(this: &mut Self) -> Mutations {
        unsafe { SerializeObserver::flush(&mut this.inner) }
    }
}

impl<'ob, S: ?Sized, D> StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = String>,
{
    /// See [`String::as_mut_str`].
    pub fn as_mut_str(&mut self) -> &mut StrObserver<'ob, StringObserverState, S, Succ<D>> {
        &mut self.inner
    }

    delegate_methods! { untracked_mut() as String =>
        pub fn push_str(&mut self, string: &str);
        pub fn extend_from_within<R>(&mut self, src: R) where R: RangeBounds<usize>;
        pub fn reserve(&mut self, additional: usize);
        pub fn reserve_exact(&mut self, additional: usize);
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError>;
        pub fn shrink_to_fit(&mut self);
        pub fn shrink_to(&mut self, min_capacity: usize);
        pub fn push(&mut self, ch: char);
    }

    /// See [`String::truncate`].
    pub fn truncate(&mut self, len: usize) {
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        state.mark_truncate(value.as_str(), len);
        value.truncate(len);
    }

    /// See [`String::pop`].
    pub fn pop(&mut self) -> Option<char> {
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        let ch = value.pop()?;
        if value.len() < state.append_index {
            state.truncate_len += ch.truncate_len();
            state.append_index = value.len();
        }
        Some(ch)
    }

    /// See [`String::remove`].
    pub fn remove(&mut self, idx: usize) -> char {
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        state.mark_truncate(value.as_str(), idx);
        value.remove(idx)
    }

    /// See [`String::retain`].
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(char) -> bool,
    {
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        let append_index = state.append_index;
        let mut byte_offset = 0;
        let mut first_removed: Option<usize> = None;
        let mut chars_in_range = 0usize;
        value.retain(|ch| {
            let kept = f(ch);
            if byte_offset < append_index {
                if !kept && first_removed.is_none() {
                    first_removed = Some(byte_offset);
                }
                if first_removed.is_some() {
                    chars_in_range += ch.truncate_len();
                }
            }
            byte_offset += ch.len_utf8();
            kept
        });
        if let Some(idx) = first_removed {
            state.truncate_len += chars_in_range;
            state.append_index = idx;
        }
    }

    /// See [`String::insert`].
    pub fn insert(&mut self, idx: usize, ch: char) {
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        state.mark_truncate(value.as_str(), idx);
        value.insert(idx, ch);
    }

    /// See [`String::insert_str`].
    pub fn insert_str(&mut self, idx: usize, string: &str) {
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        state.mark_truncate(value.as_str(), idx);
        value.insert_str(idx, string);
    }

    /// See [`String::as_mut_vec`].
    ///
    /// ## Safety
    ///
    /// See [`String::as_mut_vec`] for safety requirements.
    pub unsafe fn as_mut_vec(&mut self) -> ShallowMut<'_, Vec<u8>, StringObserverState> {
        let inner = unsafe { (*self.inner.ptr).as_deref_mut().as_mut_vec() };
        ShallowMut::new(inner, &raw mut self.inner.state)
    }

    /// See [`String::split_off`].
    pub fn split_off(&mut self, at: usize) -> String {
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        state.mark_truncate(value.as_str(), at);
        value.split_off(at)
    }

    /// See [`String::clear`].
    pub fn clear(&mut self) {
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        state.mark_truncate(value.as_str(), 0);
        value.clear();
    }

    /// See [`String::drain`].
    pub fn drain<R>(&mut self, range: R) -> Drain<'_>
    where
        R: RangeBounds<usize>,
    {
        let start_index = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        state.mark_truncate(value.as_str(), start_index);
        value.drain(range)
    }

    /// See [`String::replace_range`].
    pub fn replace_range<R>(&mut self, range: R, replace_with: &str)
    where
        R: RangeBounds<usize>,
    {
        let start_index = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let state = &mut self.inner.state;
        let value = (*self.inner.ptr).as_deref_mut();
        state.mark_truncate(value.as_str(), start_index);
        value.replace_range(range, replace_with);
    }
}

impl<'ob, S: ?Sized, D> AddAssign<&str> for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = String>,
{
    fn add_assign(&mut self, rhs: &str) {
        self.untracked_mut().add_assign(rhs);
    }
}

impl<'ob, S: ?Sized, D, U> Extend<U> for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = String>,
    String: Extend<U>,
{
    fn extend<I: IntoIterator<Item = U>>(&mut self, other: I) {
        self.untracked_mut().extend(other);
    }
}

impl<'ob, S: ?Sized, D, I> Index<I> for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = String>,
    I: SliceIndex<str>,
{
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        self.untracked_ref().index(index)
    }
}

impl<'ob, S: ?Sized, D, I> IndexMut<I> for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = String>,
    I: SliceIndex<str>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        self.tracked_mut().index_mut(index)
    }
}

impl<'ob, S: ?Sized, D> Write for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = String>,
{
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.untracked_mut().write_str(s)
    }

    fn write_char(&mut self, c: char) -> std::fmt::Result {
        self.untracked_mut().write_char(c)
    }
}

impl<'ob, S: ?Sized, D> Debug for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = String>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("StringObserver").field(&self.untracked_ref()).finish()
    }
}

impl<'ob, S: ?Sized, D> Display for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = String>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.untracked_ref(), f)
    }
}

macro_rules! generic_impl_partial_eq {
    ($($(#[$meta:meta])* impl $([$($gen:tt)*])? PartialEq<$ty:ty> for String);* $(;)?) => {
        $(
            $(#[$meta])*
            impl<'ob, $($($gen)*,)? S, D> PartialEq<$ty> for StringObserver<'ob, S, D>
            where
                D: Unsigned,
                S: AsDeref<D, Target = String>,
                String: PartialEq<$ty>,
            {
                fn eq(&self, other: &$ty) -> bool {
                    self.untracked_ref().eq(other)
                }
            }
        )*
    };
}

generic_impl_partial_eq! {
    impl PartialEq<String> for String;
    impl ['a, U: ?Sized] PartialEq<&'a U> for String;
    impl ['a, U: ToOwned + ?Sized] PartialEq<Cow<'a, U>> for String;
    #[rustversion::since(1.91)]
    impl PartialEq<std::path::Path> for String;
    #[rustversion::since(1.91)]
    impl PartialEq<std::path::PathBuf> for String;
}

impl<'ob, S1, S2, D1, D2> PartialEq<StringObserver<'ob, S2, D2>> for StringObserver<'ob, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1, Target = String>,
    S2: AsDeref<D2, Target = String>,
{
    fn eq(&self, other: &StringObserver<'ob, S2, D2>) -> bool {
        self.untracked_ref().eq(other.untracked_ref())
    }
}

impl<'ob, S, D> Eq for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = String>,
{
}

impl<'ob, S, D> PartialOrd<String> for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = String>,
{
    fn partial_cmp(&self, other: &String) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other)
    }
}

impl<'ob, S1, S2, D1, D2> PartialOrd<StringObserver<'ob, S2, D2>> for StringObserver<'ob, S1, D1>
where
    D1: Unsigned,
    D2: Unsigned,
    S1: AsDeref<D1, Target = String>,
    S2: AsDeref<D2, Target = String>,
{
    fn partial_cmp(&self, other: &StringObserver<'ob, S2, D2>) -> Option<std::cmp::Ordering> {
        self.untracked_ref().partial_cmp(other.untracked_ref())
    }
}

impl<'ob, S, D> Ord for StringObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = String>,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.untracked_ref().cmp(other.untracked_ref())
    }
}

impl Observe for String {
    type Observer<'ob, S, D>
        = StringObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ref_observe! {
    impl RefObserve for String;
}

#[cfg(test)]
mod tests {
    use muon_test_utils::*;
    use serde_json::json;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_mutation_returns_none() {
        let mut s = String::from("hello");
        let mut ob = s.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_on_deref_mut() {
        let mut s = String::from("hello");
        let mut ob = s.__observe();
        ob.clear();
        ob.push_str("world"); // append after replace should have no effect
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("world"))));
    }

    #[test]
    fn append_with_push() {
        let mut s = String::from("a");
        let mut ob = s.__observe();
        ob.push('b');
        ob.push('c');
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("bc"))));
    }

    #[test]
    fn append_with_push_str() {
        let mut s = String::from("foo");
        let mut ob = s.__observe();
        ob.push_str("bar");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("bar"))));
    }

    #[test]
    fn append_with_add_assign() {
        let mut s = String::from("foo");
        let mut ob = s.__observe();
        ob += "bar";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("bar"))));
    }

    #[test]
    fn append_empty_string() {
        let mut s = String::from("foo");
        let mut ob = s.__observe();
        ob.push_str("");
        ob += "";
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_after_append() {
        let mut s = String::from("abc");
        let mut ob = s.__observe();
        ob.push_str("def");
        *ob.tracked_mut() = String::from("xyz");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("xyz"))));
    }

    #[test]
    fn truncate() {
        let mut s = String::from("你好，世界！");
        let mut ob = s.__observe();
        ob.truncate("你好".len());
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 4)));
    }

    #[test]
    fn pop_as_truncate() {
        let mut s = String::from("你好，世界！");
        let mut ob = s.__observe();
        ob.pop();
        ob.pop();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 2)));
    }

    #[test]
    fn pop_after_append() {
        let mut s = String::from("你好！");
        let mut ob = s.__observe();
        ob.push_str("世界！");
        ob.pop();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("世界"))));
    }

    #[test]
    fn append_after_pop() {
        let mut s = String::from("你好，世界！");
        let mut ob = s.__observe();
        ob.pop();
        ob.push('~');
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 1), append!(_, json!("~")))));
    }

    #[test]
    fn remove_before_append_index() {
        let mut s = String::from("你好，世界！");
        let mut ob = s.__observe();
        assert_eq!(ob.remove("你好".len()), '，');
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!("世界！")))));
    }

    #[test]
    fn remove_at_append_index() {
        let mut s = String::from("你好，世界！");
        let mut ob = s.__observe();
        assert_eq!(ob.remove("你好，世界".len()), '！');
        assert_eq!(ob.remove("你好，世".len()), '界');
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 2)));
    }

    #[test]
    fn retain_no_removal() {
        let mut s = String::from("hello");
        let mut ob = s.__observe();
        ob.retain(|_| true);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn retain_remove_from_tracked() {
        let mut s = String::from("你好，世界！");
        let mut ob = s.__observe();
        ob.retain(|c| c != '，' && c != '！');
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!("世界")))));
    }

    #[test]
    fn retain_remove_only_after_append() {
        let mut s = String::from("ab");
        let mut ob = s.__observe();
        ob.push_str("cd");
        ob.retain(|c| c != 'c');
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("d"))));
    }

    #[test]
    fn retain_remove_all() {
        let mut s = String::from("hello");
        let mut ob = s.__observe();
        ob.retain(|_| false);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!(""))));
    }

    #[test]
    fn retain_straddles_append_index() {
        let mut s = String::from("ab");
        let mut ob = s.__observe();
        ob.push_str("cd");
        ob.retain(|c| c != 'b' && c != 'd');
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 1), append!(_, json!("c")))));
    }

    #[test]
    fn write_str_appends() {
        use std::fmt::Write;
        let mut s = String::from("foo");
        let mut ob = s.__observe();
        write!(ob, "bar{}", 42).unwrap();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("bar42"))));
    }
}
