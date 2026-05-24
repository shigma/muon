use muon::adapter::Json;
use muon::{Observe, observe};
use muon_test_utils::*;
use serde::Serialize;
use serde_json::json;

#[derive(Serialize, Observe)]
struct Simple {
    x: i32,
    y: String,
}

#[test]
fn no_mutation_returns_none() {
    let mut s = Simple {
        x: 1,
        y: "hello".into(),
    };
    let Json(mutation) = observe!(s => {}).unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn single_field_mutation() {
    let mut s = Simple {
        x: 10,
        y: "hello".into(),
    };
    let Json(mutation) = observe!(s => {
        s.x = 20;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(x, json!(20))));
}

#[test]
fn multiple_field_mutations_batch() {
    let mut s = Simple { x: 1, y: "a".into() };
    let Json(mutation) = observe!(s => {
        s.x = 2;
        s.y.push_str("b");
    })
    .unwrap();
    assert_eq!(mutation, Some(batch!(_, replace!(x, json!(2)), append!(y, json!("b")))));
}

#[test]
fn full_replace_via_deref_mut() {
    let mut s = Simple { x: 1, y: "a".into() };
    let Json(mutation) = observe!(s => {
        *s = Simple { x: 99, y: "z".into() };
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!({"x": 99, "y": "z"}))));
}

#[derive(Serialize, Observe)]
struct WithRename {
    #[serde(rename = "alpha")]
    a: i32,
    b: i32,
}

#[test]
fn serde_rename_path_segments() {
    let mut w = WithRename { a: 1, b: 2 };
    let Json(mutation) = observe!(w => {
        w.a = 10;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(alpha, json!(10))));
}

#[derive(Serialize, Observe)]
#[serde(rename_all = "camelCase")]
struct WithRenameAll {
    foo_bar: i32,
    baz_qux: i32,
}

#[test]
fn serde_rename_all_path_segments() {
    let mut w = WithRenameAll { foo_bar: 1, baz_qux: 2 };
    let Json(mutation) = observe!(w => {
        w.foo_bar = 10;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(fooBar, json!(10))));
}

#[derive(Serialize, Observe)]
struct Inner {
    c: i32,
    d: i32,
}

#[derive(Serialize, Observe)]
struct WithFlatten {
    a: i32,
    #[serde(flatten)]
    inner: Inner,
}

#[test]
fn serde_flatten_extends() {
    let mut w = WithFlatten {
        a: 1,
        inner: Inner { c: 3, d: 4 },
    };
    let Json(mutation) = observe!(w => {
        w.inner.c = 30;
    })
    .unwrap();
    // Flattened field's path should NOT be nested under "inner"
    assert_eq!(mutation, Some(replace!(c, json!(30))));
}

#[derive(Serialize, Observe)]
struct WithSkipIf {
    #[serde(skip_serializing_if = "Option::is_none")]
    val: Option<i32>,
    other: i32,
}

#[test]
fn serde_skip_serializing_if_delete() {
    let mut w = WithSkipIf {
        val: Some(42),
        other: 10,
    };
    let Json(mutation) = observe!(w => {
        w.val = None;
    })
    .unwrap();
    assert_eq!(mutation, Some(delete!(val)));
}

#[test]
fn serde_skip_serializing_if_replace() {
    let mut w = WithSkipIf { val: None, other: 10 };
    let Json(mutation) = observe!(w => {
        let _ = w.val.insert(42);
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(val, json!(42))));
}

#[derive(Serialize, Observe)]
struct WithSkip {
    a: i32,
    #[muon(skip)]
    b: i32,
}

#[test]
fn muon_skip_not_tracked() {
    let mut w = WithSkip { a: 1, b: 2 };
    let Json(mutation) = observe!(w => {
        w.b = 99;
    })
    .unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn muon_skip_with_tracked() {
    let mut w = WithSkip { a: 1, b: 2 };
    let Json(mutation) = observe!(w => {
        w.a = 10;
        w.b = 99;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!({"a": 10, "b": 99}))));
}

#[derive(Serialize, Observe)]
struct Outer {
    inner: Inner,
    x: i32,
}

#[test]
fn nested_struct_observation() {
    let mut o = Outer {
        inner: Inner { c: 1, d: 2 },
        x: 10,
    };
    let Json(mutation) = observe!(o => {
        o.inner.c = 100;
        o.x = 20;
    })
    .unwrap();
    // Single inner mutation gets flattened (no Batch wrapper for single mutation)
    assert_eq!(
        mutation,
        Some(batch!(_, replace!(inner.c, json!(100)), replace!(x, json!(20))))
    );
}

#[derive(Serialize, Observe)]
struct SingleTuple(String);

#[test]
fn tuple_struct_single_field() {
    let mut t = SingleTuple("hello".into());
    let Json(mutation) = observe!(t => {
        t.0.push_str(" world");
    })
    .unwrap();
    // Single unnamed field extends directly (no segment)
    assert_eq!(mutation, Some(append!(_, json!(" world"))));
}

#[derive(Serialize, Observe)]
struct MultiTuple(i32, String);

#[test]
fn tuple_struct_multi_field() {
    let mut t = MultiTuple(1, "hello".into());
    let Json(mutation) = observe!(t => {
        t.0 = 42;
        t.1.push_str("!");
    })
    .unwrap();
    assert_eq!(
        mutation,
        Some(batch!(_, replace!(0, json!(42)), append!(1, json!("!"))))
    );
}

#[test]
fn flush_resets_state() {
    let mut s = Simple { x: 1, y: "a".into() };
    let Json(mutation1) = observe!(s => {
        s.x = 2;
    })
    .unwrap();
    assert!(mutation1.is_some());

    // Second observe with no changes should return None
    let Json(mutation2) = observe!(s => {}).unwrap();
    assert!(mutation2.is_none());
}

#[derive(Serialize, Observe)]
struct AllSkipped {
    #[muon(skip)]
    a: i32,
    #[muon(skip)]
    b: String,
}

#[test]
fn all_fields_skipped_noop() {
    let mut a = AllSkipped {
        a: 1,
        b: "hello".into(),
    };
    let Json(mutation) = observe!(a => {
        a.a = 999;
        a.b = "changed".into();
    })
    .unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn serde_flatten_with_normal_field() {
    let mut w = WithFlatten {
        a: 1,
        inner: Inner { c: 3, d: 4 },
    };
    let Json(mutation) = observe!(w => {
        w.a = 10;
        w.inner.c = 30;
    })
    .unwrap();
    assert_eq!(
        mutation,
        Some(batch!(_, replace!(a, json!(10)), replace!(c, json!(30))))
    );
}

#[derive(Serialize, Observe)]
struct WithVec {
    items: Vec<i32>,
}

#[test]
fn vec_field_append() {
    let mut w = WithVec { items: vec![1, 2] };
    let Json(mutation) = observe!(w => {
        w.items.push(3);
    })
    .unwrap();
    assert_eq!(mutation, Some(append!(items, json!([3]))));
}
