use muon::adapter::Json;
use muon::{Observe, observe};
use muon_test_utils::*;
use serde::Serialize;
use serde_json::json;

// B2: snapshot tuple struct (chat-rs `PrefixedFileId`).
#[derive(Serialize, Observe, Clone, Copy, PartialEq)]
#[muon(snapshot)]
struct PrefixedFileId(pub u64);

#[derive(Serialize, Observe, Clone, PartialEq)]
#[muon(snapshot)]
struct Pair(i32, String);

// B2: snapshot tuple variant.
#[derive(Serialize, Observe, Clone, PartialEq)]
#[muon(snapshot)]
enum Shape {
    Point,
    Circle(u32),
    Rect(u32, u32),
}

// B3: snapshot struct with `Vec` / `String` fields.
#[derive(Serialize, Observe, Clone, PartialEq)]
#[muon(snapshot)]
struct Tagged {
    name: String,
    tags: Vec<String>,
}

#[derive(Serialize, Observe)]
struct Holder {
    id: PrefixedFileId,
    pair: Pair,
    shape: Shape,
    tagged: Tagged,
}

fn holder() -> Holder {
    Holder {
        id: PrefixedFileId(0),
        pair: Pair(1, "a".into()),
        shape: Shape::Rect(1, 2),
        tagged: Tagged {
            name: "n".into(),
            tags: vec!["x".into()],
        },
    }
}

#[test]
fn no_change() {
    let mut h = holder();
    let Json(mutation) = observe!(h => {
        h.id = PrefixedFileId(0);
        h.pair = Pair(1, "a".into());
        h.shape = Shape::Rect(1, 2);
        h.tagged = Tagged { name: "n".into(), tags: vec!["x".into()] };
    })
    .unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn tuple_struct_single_field() {
    let mut h = holder();
    let id = 1u64;
    let Json(mutation) = observe!(h => {
        h.id = PrefixedFileId(id);
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(id, json!(id))));
}

#[test]
fn tuple_struct_whole_replace() {
    let mut h = holder();
    let Json(mutation) = observe!(h => {
        h.pair = Pair(2, "b".into());
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(pair, json!([2, "b"]))));
}

#[test]
fn tuple_variant() {
    let mut h = holder();
    let Json(mutation) = observe!(h => {
        h.shape = Shape::Rect(3, 4);
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(shape, json!({ "Rect": [3, 4] }))));

    let Json(mutation) = observe!(h => {
        h.shape = Shape::Circle(5);
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(shape, json!({ "Circle": 5 }))));

    let Json(mutation) = observe!(h => {
        h.shape = Shape::Point;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(shape, json!("Point"))));
}

#[test]
fn vec_and_string_fields() {
    let mut h = holder();
    let Json(mutation) = observe!(h => {
        h.tagged = Tagged { name: "m".into(), tags: vec!["y".into()] };
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(tagged, json!({ "name": "m", "tags": ["y"] }))));
}
