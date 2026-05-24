// Add leading colons to std imports to avoid rustfmt inserting newlines
use ::std::ops::{Deref, DerefMut};
#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;

#[rustfmt::skip]
#[derive(Serialize, Observe)]
// FIXME: #[muon(derive(PartialEq))]
pub struct Foo<T> {
    #[serde(flatten)]
    #[muon(deref)]
    a: Vec<T>,
    b: i32,
}

impl<T> Deref for Foo<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.a
    }
}

impl<T> DerefMut for Foo<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.a
    }
}

#[rustfmt::skip]
#[derive(Serialize, Observe)]
pub struct Bar(#[muon(deref, shallow)] Qux, i32);

impl Deref for Bar {
    type Target = Qux;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Bar {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[rustfmt::skip]
#[derive(Serialize, Observe)]
pub struct Qux(#[muon(deref)] pub i32);

impl Deref for Qux {
    type Target = i32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Qux {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
