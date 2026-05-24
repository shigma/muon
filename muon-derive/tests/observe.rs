use muon::adapter::Json;
use muon::{Observe, observe};
use muon_test_utils::*;
use serde::Serialize;
use serde_json::json;

#[derive(Serialize, Observe)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Serialize, Observe)]
struct Nested {
    pos: Point,
    label: String,
}

#[test]
fn arm_form_basic() {
    let mut p = Point { x: 1, y: 2 };
    let Json(mutation) = observe!(p => {
        p.x = 10;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(x, json!(10))));
}

#[test]
fn closure_form() {
    let cb = observe!(|p: &mut Point| {
        p.x = 10;
    });
    let mut p = Point { x: 1, y: 2 };
    let Json(mutation) = cb(&mut p).unwrap();
    assert_eq!(mutation, Some(replace!(x, json!(10))));
}

#[test]
fn assignment_tracks_mutation() {
    let mut p = Point { x: 0, y: 0 };
    let Json(mutation) = observe!(p => {
        p.x = 42;
        p.y = 99;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!({"x": 42, "y": 99}))));
}

#[test]
fn comparison_works() {
    let mut p = Point { x: 5, y: 10 };
    let Json(mutation) = observe!(p => {
        if p.x == 5 {
            p.y = 20;
        }
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(y, json!(20))));
}

#[test]
fn comparison_no_mutation() {
    let mut p = Point { x: 5, y: 10 };
    let Json(mutation) = observe!(p => {
        if p.x == 999 {
            p.y = 20;
        }
    })
    .unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn wildcard_pattern() {
    let result: Result<Json, serde_json::Error> = observe!(_ => {
        let _ = 1 + 1;
    });
    let Json(mutation) = result.unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn nested_field_access() {
    let mut n = Nested {
        pos: Point { x: 0, y: 0 },
        label: "start".into(),
    };
    let Json(mutation) = observe!(n => {
        n.pos.x = 100;
        n.label.push_str("!");
    })
    .unwrap();
    // Single inner mutation gets flattened (path combined)
    assert_eq!(
        mutation,
        Some(batch!(_, replace!(pos.x, json!(100)), append!(label, json!("!"))))
    );
}

#[test]
fn no_mutation_returns_none() {
    let mut p = Point { x: 1, y: 2 };
    let Json(mutation) = observe!(p => {}).unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn closure_no_mutation() {
    let cb = observe!(|p: &mut Point| {});
    let mut p = Point { x: 1, y: 2 };
    let Json(mutation) = cb(&mut p).unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn closure_multiple_calls() {
    let cb = observe!(|p: &mut Point| {
        p.x += 1;
    });

    let mut p = Point { x: 0, y: 0 };

    let Json(mutation1) = cb(&mut p).unwrap();
    assert_eq!(mutation1, Some(replace!(x, json!(1))));

    let Json(mutation2) = cb(&mut p).unwrap();
    assert_eq!(mutation2, Some(replace!(x, json!(2))));
}

#[test]
fn compound_assignment() {
    let mut p = Point { x: 10, y: 20 };
    let Json(mutation) = observe!(p => {
        p.x += 5;
        p.y -= 3;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!({"x": 15, "y": 17}))));
}
