#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;

#[rustfmt::skip]
#[derive(Serialize, Observe)]
#[serde(bound = "S: Serialize, U: Serialize, V: Serialize")]
pub struct Foo<'a, S, T, U, V, const N: usize> {
    #[serde(serialize_with = "serialize_mut_array")]
    a: &'a mut [S; N],
    #[serde(skip)]
    pub b: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c: Option<U>,
    #[muon(shallow)]
    pub d: V,
}

#[rustfmt::skip]
fn serialize_mut_array<T, S, const N: usize>(a: &&mut [T; N], serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: serde::Serializer,
{
    <[_]>::serialize(&**a, serializer)
}
