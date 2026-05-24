#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;

#[rustfmt::skip]
#[derive(PartialEq, Eq, PartialOrd, Ord, Serialize, Observe)]
#[muon(derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum Foo {
    A,
    B(),
    C {},
}

#[rustfmt::skip]
#[derive(Serialize, Observe)]
#[muon(snapshot)]
pub enum Bar {
    A,
    B(),
    C {},
}
