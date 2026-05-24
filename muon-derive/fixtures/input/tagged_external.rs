#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;

#[rustfmt::skip]
#[derive(Serialize, Observe)]
#[serde(rename_all = "lowercase")]
pub enum Foo<S, T, U> where T: Clone {
    A(#[muon(skip)] S),
    B(u32, U),
    #[serde(rename_all = "UPPERCASE")]
    #[serde(rename = "OwO")]
    C {
        #[serde(skip)]
        bar: Option<T>,
        #[serde(rename = "QwQ")]
        qux: Qux,
    },
    D,
    E(),
    F {},
}

#[rustfmt::skip]
#[derive(Serialize, Observe)]
pub struct Qux {}
