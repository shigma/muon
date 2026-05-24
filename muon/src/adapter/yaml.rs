use serde_yaml_ng::value::Serializer;
use serde_yaml_ng::{Error, Value};

use crate::{Adapter, Mutation, Mutations, PathSegment};

/// YAML adapter for muon mutation serialization.
///
/// [`Yaml`] implements the [`Adapter`] trait using [`serde_yaml_ng::Value`].
///
/// ## Example
///
/// ```
/// use muon::adapter::Yaml;
/// use muon::{Observe, observe};
/// use serde::Serialize;
///
/// #[derive(Serialize, Observe)]
/// struct Config {
///     host: String,
///     port: u16,
///     tags: Vec<String>,
/// }
///
/// let mut config = Config {
///     host: "localhost".to_string(),
///     port: 8080,
///     tags: vec!["web".to_string()],
/// };
///
/// let Yaml(mutation) = observe!(config => {
///     config.port = 8081;
///     config.tags.push("api".to_string());
/// }).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Yaml(pub Option<Mutation<Value>>);

impl Adapter for Yaml {
    type Value = Value;
    type Error = Error;

    fn from_mutations(mutation: Mutations) -> Result<Self, Self::Error> {
        Ok(Self(
            mutation
                .into_inner()
                .map(|mutation| mutation.try_map(&mut |value| erased_serde::serialize(&*value, Serializer)))
                .transpose()?,
        ))
    }

    fn get_mut<'a>(
        value: &'a mut Self::Value,
        segment: &PathSegment,
        allow_create: bool,
    ) -> Option<&'a mut Self::Value> {
        match (value, segment) {
            (Value::Sequence(vec), PathSegment::Positive(index)) => vec.get_mut(*index),
            (Value::Sequence(vec), PathSegment::Negative(index)) => {
                vec.len().checked_sub(*index).and_then(|i| vec.get_mut(i))
            }
            (Value::Mapping(map), PathSegment::String(key)) => {
                if allow_create {
                    Some(map.entry(Value::String(key.to_string())).or_insert(Value::Null))
                } else {
                    map.get_mut(&**key)
                }
            }
            _ => None,
        }
    }

    #[cfg(feature = "delete")]
    fn delete(value: &mut Self::Value, segment: &PathSegment) -> Option<Self::Value> {
        match (value, segment) {
            (Value::Mapping(map), PathSegment::String(key)) => map.remove(&**key),
            _ => None,
        }
    }

    #[cfg(feature = "append")]
    fn append(value: &mut Self::Value, append_value: Self::Value) -> Option<usize> {
        match (value, append_value) {
            (Value::String(lhs), Value::String(rhs)) => {
                let len = rhs.chars().count();
                *lhs += &rhs;
                Some(len)
            }
            (Value::Sequence(lhs), Value::Sequence(rhs)) => {
                let len = rhs.len();
                lhs.extend(rhs);
                Some(len)
            }
            _ => None,
        }
    }

    #[cfg(feature = "append")]
    fn len(value: &Self::Value) -> Option<usize> {
        match value {
            Value::String(str) => Some(str.chars().count()),
            Value::Sequence(vec) => Some(vec.len()),
            _ => None,
        }
    }

    #[cfg(feature = "truncate")]
    fn truncate(value: &mut Self::Value, mut truncate_len: usize) -> Option<usize> {
        match value {
            Value::String(str) => {
                let mut chars = str.char_indices();
                let mut new_len = str.len();
                while truncate_len > 0
                    && let Some((index, _)) = chars.next_back()
                {
                    truncate_len -= 1;
                    new_len = index;
                }
                str.truncate(new_len);
                Some(truncate_len)
            }
            Value::Sequence(vec) => {
                let actual_len = vec.len();
                let new_len = actual_len.saturating_sub(truncate_len);
                vec.truncate(new_len);
                Some(truncate_len.saturating_sub(actual_len))
            }
            _ => None,
        }
    }
}
