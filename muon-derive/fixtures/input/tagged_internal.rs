#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;

#[rustfmt::skip]
#[derive(Serialize, Observe)]
#[serde(tag = "type")]
pub enum Foo<const N: usize> {
    #[serde(serialize_with = "<[_]>::serialize")]
    A([u32; N]),
    // #[serde(tag = "...")] cannot be used with tuple variants
    // B(u32, u32),
    C {
        #[serde(skip_serializing_if = "String::is_empty")]
        bar: String,
        #[serde(flatten)]
        qux: Qux,
    },
}

#[rustfmt::skip]
#[derive(Serialize, Observe)]
pub struct Qux {}
