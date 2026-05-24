#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;

#[rustfmt::skip]
#[derive(Serialize, Observe)]
#[serde(tag = "type", content = "data")]
pub enum Foo<'i> {
    A(u32),
    B(u32, u32),
    C {
        bar: &'i mut String,
    },
}
