// Add leading colons to std imports to avoid rustfmt inserting newlines
use ::std::collections::HashMap;
use ::std::fmt::Display;
#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;

#[rustfmt::skip]
#[derive(Debug, Serialize, Observe)]
#[muon(derive(Debug, Display))]
#[serde(rename_all = "UPPERCASE")]
pub struct Foo {
    r#a: i32,
    #[serde(rename = "bar")]
    b: String,
    #[serde(flatten)]
    c: HashMap<String, i32>,
}

impl Display for Foo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Foo {{ a: {}, b: {} }}", self.a, self.b)
    }
}

#[rustfmt::skip]
#[derive(Serialize, Observe)]
#[muon(expose)]
pub struct Bar(i32);

#[rustfmt::skip]
#[derive(PartialEq, Eq, PartialOrd, Ord, Serialize, Observe)]
#[muon(derive(Debug, PartialEq, Eq, PartialOrd, Ord))]
pub struct Baz(i32, String);
