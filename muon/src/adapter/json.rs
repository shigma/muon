use serde_json::value::Serializer;
use serde_json::{Error, Value};

use crate::{Adapter, Mutation, Mutations, PathSegment};

/// JSON adapter for muon mutation serialization.
///
/// [`Json`] implements the [`Adapter`] trait using [`serde_json::Value`].
///
/// ## Example
///
/// ```
/// use muon::adapter::Json;
/// use muon::{Observe, observe};
/// use serde::Serialize;
///
/// #[derive(Serialize, Observe)]
/// struct Data {
///     value: i32,
/// }
///
/// let mut data = Data { value: 42 };
/// let Json(mutation) = observe!(data => {
///     data.value += 1;
/// }).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Json(pub Option<Mutation<Value>>);

impl Adapter for Json {
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
            (Value::Array(vec), PathSegment::Positive(index)) => vec.get_mut(*index),
            (Value::Array(vec), PathSegment::Negative(index)) => {
                vec.len().checked_sub(*index).and_then(|i| vec.get_mut(i))
            }
            (Value::Object(map), PathSegment::String(key)) => {
                if allow_create {
                    Some(map.entry(&**key).or_insert(Value::Null))
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
            (Value::Object(map), PathSegment::String(key)) => map.remove(&**key),
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
            (Value::Array(lhs), Value::Array(rhs)) => {
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
            Value::Array(vec) => Some(vec.len()),
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
            Value::Array(vec) => {
                let actual_len = vec.len();
                let new_len = actual_len.saturating_sub(truncate_len);
                vec.truncate(new_len);
                Some(truncate_len.saturating_sub(actual_len))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod test {
    use muon_test_utils::*;
    use serde_json::json;

    use super::*;
    use crate::MutationError;

    #[test]
    fn apply_set() {
        let mut value = json!({"a": 1});
        Json::mutate(&mut value, replace!(_, json!({})), &mut Default::default()).unwrap();
        assert_eq!(value, json!({}));

        let mut value = json!({});
        Json::mutate(&mut value, replace!(a, json!(1)), &mut Default::default()).unwrap();
        assert_eq!(value, json!({"a": 1}));

        let mut value = json!({"a": 1});
        Json::mutate(&mut value, replace!(a, json!(2)), &mut Default::default()).unwrap();
        assert_eq!(value, json!({"a": 2}));

        let error = Json::mutate(&mut json!({}), replace!(a.b, json!(3)), &mut Default::default()).unwrap_err();
        assert_eq!(
            error,
            MutationError::IndexError {
                path: vec!["a".into()].into()
            }
        );

        let error = Json::mutate(&mut json!({"a": 1}), replace!(a.b, json!(3)), &mut Default::default()).unwrap_err();
        assert_eq!(
            error,
            MutationError::IndexError {
                path: vec!["a".into(), "b".into()].into(),
            }
        );

        let error = Json::mutate(&mut json!({"a": []}), replace!(a.b, json!(3)), &mut Default::default()).unwrap_err();
        assert_eq!(
            error,
            MutationError::IndexError {
                path: vec!["a".into(), "b".into()].into(),
            }
        );

        let mut value = json!({"a": {}});
        Json::mutate(&mut value, replace!(a.b, json!(3)), &mut Default::default()).unwrap();
        assert_eq!(value, json!({"a": {"b": 3}}));
    }

    #[test]
    fn apply_append() {
        let mut value = json!("2");
        Json::mutate(&mut value, append!(_, json!("34")), &mut Default::default()).unwrap();
        assert_eq!(value, json!("234"));

        let mut value = json!([2]);
        Json::mutate(&mut value, append!(_, json!(["3", "4"])), &mut Default::default()).unwrap();
        assert_eq!(value, json!([2, "3", "4"]));

        let error = Json::mutate(&mut json!(""), append!(_, json!(3)), &mut Default::default()).unwrap_err();
        assert_eq!(
            error,
            MutationError::OperationError {
                path: Default::default()
            }
        );

        let error = Json::mutate(&mut json!({}), append!(_, json!("3")), &mut Default::default()).unwrap_err();
        assert_eq!(error, MutationError::OperationError { path: vec![].into() });

        let error = Json::mutate(&mut json!([]), append!(_, json!("3")), &mut Default::default()).unwrap_err();
        assert_eq!(error, MutationError::OperationError { path: vec![].into() });

        let error = Json::mutate(&mut json!(""), append!(_, json!([3])), &mut Default::default()).unwrap_err();
        assert_eq!(error, MutationError::OperationError { path: vec![].into() });
    }

    #[test]
    fn apply_truncate() {
        let mut value = json!("Hello, World!");
        Json::mutate(&mut value, truncate!(_, 8), &mut Default::default()).unwrap();
        assert_eq!(value, json!("Hello"));

        let mut value = json!("我是谁");
        Json::mutate(&mut value, truncate!(_, 2), &mut Default::default()).unwrap();
        assert_eq!(value, json!("我"));

        let error = Json::mutate(&mut json!("Hello, World!"), truncate!(_, 20), &mut Default::default()).unwrap_err();
        assert_eq!(
            error,
            MutationError::TruncateError {
                path: vec![].into(),
                actual_len: 13,
                truncate_len: 20,
            }
        );
    }

    #[test]
    fn apply_batch() {
        let mut value = json!({"a": {"b": {"c": {}}}});
        Json::mutate(&mut value, batch!(_,), &mut Default::default()).unwrap();
        assert_eq!(value, json!({"a": {"b": {"c": {}}}}));

        let mut value = json!({"a": {"b": {"c": "1"}}});
        let error = Json::mutate(&mut value, batch!(a.d,), &mut Default::default()).unwrap_err();
        assert_eq!(
            error,
            MutationError::IndexError {
                path: vec!["a".into(), "d".into()].into(),
            }
        );

        let mut value = json!({"a": {"b": {"c": "1"}}});
        Json::mutate(
            &mut value,
            batch!(a, append!(b.c, json!("2")), replace!(d, json!(3))),
            &mut Default::default(),
        )
        .unwrap();
        assert_eq!(value, json!({"a": {"b": {"c": "12"}, "d": 3}}));
    }
}
