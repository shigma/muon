//! Observer implementation for [`Url`].

use std::fmt::Display;
use std::net::IpAddr;

use url::{ParseError, Url};

use crate::Observe;
use crate::helper::macros::default_impl_ro_observe;
use crate::helper::shallow::shallow_observer;
use crate::helper::{AsDeref, AsDerefMut, QuasiObserver, Unsigned};
use crate::impls::strings::string::StringObserverState;
use crate::observe::DefaultSpec;

shallow_observer! {
    /// Observer implementation for [`Url`].
    struct UrlObserver(pub(crate) Url, pub(crate) StringObserverState);
}

#[expect(clippy::result_unit_err)]
impl<'ob, S: ?Sized, D> UrlObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDerefMut<D, Target = Url>,
{
    fn offset(base: &str, sub: &str) -> usize {
        sub.as_ptr() as usize - base.as_ptr() as usize
    }

    /// See [`Url::set_fragment`].
    pub fn set_fragment(&mut self, fragment: Option<&str>) {
        let value = (*self.ptr).as_deref_mut();
        let str = value.as_str();
        let preserved = value.fragment().map_or(str.len(), |f| Self::offset(str, f) - 1);
        self.state.mark_truncate(str, preserved);
        value.set_fragment(fragment);
    }

    /// See [`Url::set_query`].
    pub fn set_query(&mut self, query: Option<&str>) {
        let value = (*self.ptr).as_deref_mut();
        let str = value.as_str();
        let preserved = value.query().map_or_else(
            || value.fragment().map_or(str.len(), |f| Self::offset(str, f) - 1),
            |q| Self::offset(str, q) - 1,
        );
        self.state.mark_truncate(str, preserved);
        value.set_query(query);
    }

    // TODO: query_pairs_mut

    /// See [`Url::set_path`].
    pub fn set_path(&mut self, path: &str) {
        let value = (*self.ptr).as_deref_mut();
        let str = value.as_str();
        let preserved = Self::offset(str, value.path());
        self.state.mark_truncate(str, preserved);
        value.set_path(path);
    }

    // TODO: path_segments_mut

    /// See [`Url::set_port`].
    pub fn set_port(&mut self, port: Option<u16>) -> Result<(), ()> {
        let value = (*self.ptr).as_deref_mut();
        let str = value.as_str();
        let preserved = value.host_str().map_or(0, |h| Self::offset(str, h) + h.len());
        self.state.mark_truncate(str, preserved);
        value.set_port(port)
    }

    /// See [`Url::set_host`].
    pub fn set_host(&mut self, host: Option<&str>) -> Result<(), ParseError> {
        let value = (*self.ptr).as_deref_mut();
        let str = value.as_str();
        let preserved = value.host_str().map_or(0, |h| Self::offset(str, h));
        self.state.mark_truncate(str, preserved);
        value.set_host(host)
    }

    /// See [`Url::set_ip_host`].
    pub fn set_ip_host(&mut self, address: IpAddr) -> Result<(), ()> {
        let value = (*self.ptr).as_deref_mut();
        let str = value.as_str();
        let preserved = value.host_str().map_or(0, |h| Self::offset(str, h));
        self.state.mark_truncate(str, preserved);
        value.set_ip_host(address)
    }

    /// See [`Url::set_password`].
    pub fn set_password(&mut self, password: Option<&str>) -> Result<(), ()> {
        let value = (*self.ptr).as_deref_mut();
        let str = value.as_str();
        let preserved = value.password().map_or(0, |p| Self::offset(str, p) - 1);
        self.state.mark_truncate(str, preserved);
        value.set_password(password)
    }

    /// See [`Url::set_username`].
    pub fn set_username(&mut self, username: &str) -> Result<(), ()> {
        let value = (*self.ptr).as_deref_mut();
        let str = value.as_str();
        let u = value.username();
        let preserved = if u.is_empty() { 0 } else { Self::offset(str, u) };
        self.state.mark_truncate(str, preserved);
        value.set_username(username)
    }

    /// See [`Url::set_scheme`].
    pub fn set_scheme(&mut self, scheme: &str) -> Result<(), ()> {
        let value = (*self.ptr).as_deref_mut();
        self.state.mark_truncate(value.as_str(), 0);
        value.set_scheme(scheme)
    }
}

impl<'ob, S: ?Sized, D> Display for UrlObserver<'ob, S, D>
where
    D: Unsigned,
    S: AsDeref<D, Target = Url>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.untracked_ref(), f)
    }
}

impl Observe for Url {
    type Observer<'ob, S, D>
        = UrlObserver<'ob, S, D>
    where
        Self: 'ob,
        D: Unsigned,
        S: AsDerefMut<D, Target = Self> + ?Sized + 'ob;

    type Spec = DefaultSpec;
}

default_impl_ro_observe! {
    impl RoObserve for Url;
}

#[cfg(test)]
mod tests {
    use muon_test_utils::*;
    use serde_json::json;
    use url::Url;

    use crate::adapter::Json;
    use crate::helper::QuasiObserver;
    use crate::observe::{ObserveExt, SerializeObserverExt};

    #[test]
    fn no_mutation_returns_none() {
        let mut u = Url::parse("https://example.com/path").unwrap();
        let mut ob = u.__observe();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, None);
    }

    #[test]
    fn replace_on_deref_mut() {
        let mut u = Url::parse("https://example.com/path").unwrap();
        let mut ob = u.__observe();
        *ob.tracked_mut() = Url::parse("https://other.com").unwrap();
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("https://other.com/"))));
    }

    #[test]
    fn set_fragment_appends() {
        let mut u = Url::parse("https://example.com/path").unwrap();
        let mut ob = u.__observe();
        ob.set_fragment(Some("section"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("#section"))));
    }

    #[test]
    fn set_fragment_replaces_existing() {
        let mut u = Url::parse("https://example.com/path#old").unwrap();
        let mut ob = u.__observe();
        ob.set_fragment(Some("new"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!("#new")))));
    }

    #[test]
    fn remove_fragment_truncates() {
        let mut u = Url::parse("https://example.com/path#frag").unwrap();
        let mut ob = u.__observe();
        ob.set_fragment(None);
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(truncate!(_, 5)));
    }

    #[test]
    fn set_query_appends() {
        let mut u = Url::parse("https://example.com/path").unwrap();
        let mut ob = u.__observe();
        ob.set_query(Some("key=val"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(append!(_, json!("?key=val"))));
    }

    #[test]
    fn set_query_with_fragment() {
        let mut u = Url::parse("https://example.com/path#frag").unwrap();
        let mut ob = u.__observe();
        ob.set_query(Some("key=val"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 5), append!(_, json!("?key=val#frag"))))
        );
    }

    #[test]
    fn set_scheme_replaces() {
        let mut u = Url::parse("https://example.com/path").unwrap();
        let mut ob = u.__observe();
        let _ = ob.set_scheme("http");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(replace!(_, json!("http://example.com/path"))));
    }

    #[test]
    fn set_path_replaces_suffix() {
        let mut u = Url::parse("https://example.com/old").unwrap();
        let mut ob = u.__observe();
        ob.set_path("/new");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 4), append!(_, json!("/new")))));
    }

    #[test]
    fn set_host() {
        let mut u = Url::parse("https://example.com/path").unwrap();
        let mut ob = u.__observe();
        let _ = ob.set_host(Some("other.org"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 16), append!(_, json!("other.org/path"))))
        );
    }

    #[test]
    fn set_port() {
        let mut u = Url::parse("https://example.com:8080/path").unwrap();
        let mut ob = u.__observe();
        let _ = ob.set_port(Some(443));
        let Json(mutation) = ob.flush().unwrap();
        // 443 is default for https, so url crate omits it
        assert_eq!(mutation, Some(batch!(_, truncate!(_, 10), append!(_, json!("/path")))));
    }

    #[test]
    fn set_username() {
        let mut u = Url::parse("https://user@example.com/path").unwrap();
        let mut ob = u.__observe();
        let _ = ob.set_username("admin");
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 21), append!(_, json!("admin@example.com/path"))))
        );
    }

    #[test]
    fn set_password() {
        let mut u = Url::parse("https://user:secret@example.com/path").unwrap();
        let mut ob = u.__observe();
        let _ = ob.set_password(Some("pw"));
        let Json(mutation) = ob.flush().unwrap();
        assert_eq!(
            mutation,
            Some(batch!(_, truncate!(_, 24), append!(_, json!(":pw@example.com/path"))))
        );
    }
}
