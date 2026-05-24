// Add leading colons to std imports to avoid rustfmt inserting newlines
use ::std::fmt::Display;
#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;

#[rustfmt::skip]
#[derive(Serialize, Observe)]
#[serde(untagged, rename_all_fields = "UPPERCASE")]
#[muon(derive(Display))]
pub enum Foo {
    A(u32),
    B(u32, u32),
    C {
        bar: String,
    },
    D,
    E(),
    F {},
}

impl Display for Foo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Foo::A(a) => write!(f, "Foo::A({})", a),
            Foo::B(a, b) => write!(f, "Foo::B({}, {})", a, b),
            Foo::C { bar } => write!(f, "Foo::C {{ bar: {} }}", bar),
            Foo::D => write!(f, "Foo::D"),
            Foo::E() => write!(f, "Foo::E()"),
            Foo::F {} => write!(f, "Foo::F {{}}"),
        }
    }
}
