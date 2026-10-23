use muon::adapter::Json;
use muon::helper::QuasiObserver;
use muon::observe::{ObserveExt, SerializeObserverExt};
use muon::{Observe, observe};
use muon_test_utils::*;
use serde::Serialize;
use serde_json::json;

#[derive(Serialize, Debug, PartialEq, Observe)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Point,
    Origin,
}

#[test]
fn unit_variant_no_change() {
    let mut s = Shape::Point;
    let Json(mutation) = observe!(s => {}).unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn unit_variant_change_to_unit() {
    let mut s = Shape::Point;
    let Json(mutation) = observe!(s => {
        *s = Shape::Origin;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!("Origin"))));
}

#[test]
fn unit_to_field_variant() {
    let mut s = Shape::Point;
    let Json(mutation) = observe!(s => {
        *s = Shape::Circle { radius: 5.0 };
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!({"Circle": {"radius": 5.0}}))));
}

#[test]
fn field_to_unit_variant() {
    let mut s = Shape::Circle { radius: 3.0 };
    let Json(mutation) = observe!(s => {
        *s = Shape::Point;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!("Point"))));
}

#[test]
fn field_variant_inner_mutation() {
    let mut s = Shape::Rectangle {
        width: 10.0,
        height: 20.0,
    };
    let mut ob = s.__observe();
    // Use untracked_mut to access inner fields without triggering DerefMut
    if let Shape::Rectangle { width, .. } = ob.untracked_mut() {
        *width = 5.0;
    }
    let Json(mutation) = ob.flush().unwrap();
    // External tagging: variant + field combined into path
    assert_eq!(mutation, Some(replace!(Rectangle.width, json!(5.0))));
}

#[test]
fn field_variant_multiple_inner_mutations() {
    let mut s = Shape::Rectangle {
        width: 10.0,
        height: 20.0,
    };
    let mut ob = s.__observe();
    if let Shape::Rectangle { width, height } = ob.untracked_mut() {
        *width = 15.0;
        *height = 25.0;
    }
    let Json(mutation) = ob.flush().unwrap();
    // External tagging: variant + field combined via insert2
    assert_eq!(
        mutation,
        Some(replace!(_, json!({"Rectangle": {"width": 15.0, "height": 25.0}})))
    );
}

#[test]
fn field_variant_no_change() {
    let mut s = Shape::Circle { radius: 3.0 };
    let mut ob = s.__observe();
    // Read-only access through Deref (does not trigger mutation tracking)
    if let Shape::Circle { radius } = ob.untracked_ref() {
        let _ = *radius;
    }
    let Json(mutation) = ob.flush().unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn field_variant_deref_mut_replace() {
    let mut s = Shape::Circle { radius: 3.0 };
    let Json(mutation) = observe!(s => {
        *s = Shape::Circle { radius: 10.0 };
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!({"Circle": {"radius": 10.0}}))));
}

#[derive(Serialize, Observe)]
#[serde(rename_all = "snake_case")]
enum Action {
    DoSomething { value: i32, bar: i32 },
    DoNothing,
}

#[test]
fn enum_rename_all_variant() {
    let mut a = Action::DoNothing;
    let Json(mutation) = observe!(a => {
        *a = Action::DoSomething { value: 42, bar: 0 };
    })
    .unwrap();
    assert_eq!(
        mutation,
        Some(replace!(_, json!({"do_something": {"value": 42, "bar": 0}})))
    );
}

#[test]
fn enum_rename_all_inner_mutation() {
    let mut a = Action::DoSomething { value: 1, bar: 0 };
    let mut ob = a.__observe();
    if let Action::DoSomething { value, .. } = ob.untracked_mut() {
        *value = 99;
    }
    let Json(mutation) = ob.flush().unwrap();
    // External tagging with rename_all: variant + field combined
    assert_eq!(mutation, Some(replace!(do_something.value, json!(99))));
}

#[test]
fn flush_resets_state() {
    let mut s = Shape::Point;
    let Json(mutation1) = observe!(s => {
        *s = Shape::Origin;
    })
    .unwrap();
    assert!(mutation1.is_some());

    let Json(mutation2) = observe!(s => {}).unwrap();
    assert!(mutation2.is_none());
}

#[test]
fn flush_resets_field_variant() {
    let mut s = Shape::Circle { radius: 3.0 };
    let mut ob = s.__observe();

    if let Shape::Circle { radius } = ob.untracked_mut() {
        *radius = 5.0;
    }
    let Json(mutation1) = ob.flush().unwrap();
    assert!(mutation1.is_some());

    // No more changes — should be None
    let Json(mutation2) = ob.flush().unwrap();
    assert!(mutation2.is_none());
}

#[derive(Serialize, Observe)]
#[allow(dead_code)]
enum Color {
    Red,
    Green,
    Blue,
}

#[test]
fn all_unit_enum_no_change() {
    let mut c = Color::Red;
    let Json(mutation) = observe!(c => {}).unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn all_unit_enum_change() {
    let mut c = Color::Red;
    let Json(mutation) = observe!(c => {
        *c = Color::Blue;
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!("Blue"))));
}

#[derive(Serialize, Observe)]
#[serde(tag = "type")]
enum Event {
    Click { x: i32, y: i32 },
    Scroll { delta: i32 },
}

#[test]
fn internal_tag_inner_mutation() {
    let mut e = Event::Click { x: 10, y: 20 };
    let mut ob = e.__observe();
    if let Event::Click { x, .. } = ob.untracked_mut() {
        *x = 50;
    }
    let Json(mutation) = ob.flush().unwrap();
    // Internal tagging: no variant segment in path
    assert_eq!(mutation, Some(replace!(x, json!(50))));
}

#[test]
fn internal_tag_variant_change() {
    let mut e = Event::Click { x: 10, y: 20 };
    let Json(mutation) = observe!(e => {
        *e = Event::Scroll { delta: 5 };
    })
    .unwrap();
    assert_eq!(mutation, Some(replace!(_, json!({"type": "Scroll", "delta": 5}))));
}

#[derive(Serialize, Observe)]
#[allow(dead_code)]
enum Container {
    Items { list: Vec<i32> },
    Empty,
}

#[test]
fn field_variant_vec_append() {
    let mut c = Container::Items { list: vec![1, 2, 3] };
    let mut ob = c.__observe();
    if let Container::Items { list } = ob.untracked_mut() {
        list.push(4);
    }
    let Json(mutation) = ob.flush().unwrap();
    // External tagging: variant + field combined
    assert_eq!(mutation, Some(append!(Items.list, json!([4]))));
}

#[derive(Serialize, Observe)]
enum Wrapper {
    Single(i32),
    Pair(i32, String),
}

#[test]
fn tuple_variant_single_field() {
    let mut w = Wrapper::Single(10);
    let mut ob = w.__observe();
    if let Wrapper::Single(v) = ob.untracked_mut() {
        *v = 20;
    }
    let Json(mutation) = ob.flush().unwrap();
    // External tagging: single tuple field uses variant name + index
    assert_eq!(mutation, Some(replace!(Single, json!(20))));
}

#[test]
fn tuple_variant_multi_field() {
    let mut w = Wrapper::Pair(1, "hello".into());
    let mut ob = w.__observe();
    if let Wrapper::Pair(n, s) = ob.untracked_mut() {
        *n = 42;
        s.push('!');
    }
    let Json(mutation) = ob.flush().unwrap();
    // External tagging: variant + index combined via insert2
    assert_eq!(
        mutation,
        Some(batch!(Pair, replace!(0, json!(42)), append!(1, json!("!"))))
    );
}

#[derive(Serialize, Observe)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
enum RefData {
    ToolOpen,
    ToolSearch { search_urls: Vec<String> },
}

#[derive(Serialize, Observe)]
struct RefView {
    id: u32,
    #[serde(flatten)]
    data: RefData,
}

#[test]
fn flatten_unit_variant_no_change() {
    let mut v = RefView {
        id: 1,
        data: RefData::ToolOpen,
    };
    let Json(mutation) = observe!(v => {}).unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn flatten_field_variant_flush_twice() {
    let mut v = RefView {
        id: 1,
        data: RefData::ToolSearch { search_urls: vec![] },
    };
    let mut ob = v.__observe();
    let Json(mutation) = ob.flush().unwrap();
    assert_eq!(mutation, None);
    let Json(mutation) = ob.flush().unwrap();
    assert_eq!(mutation, None);
}

#[test]
fn flatten_field_variant_inner_mutation() {
    let mut v = RefView {
        id: 1,
        data: RefData::ToolSearch {
            search_urls: vec!["a".into()],
        },
    };
    let mut ob = v.__observe();
    if let RefData::ToolSearch { search_urls } = &mut ob.untracked_mut().data {
        search_urls.push("b".into());
    }
    let Json(mutation) = ob.flush().unwrap();
    assert_eq!(mutation, Some(append!(search_urls, json!(["b"]))));
}

#[derive(Serialize, Observe)]
#[serde(rename_all_fields = "camelCase")]
enum RenameFields {
    Foo {
        foo_bar: i32,
        other: i32,
    },
    #[serde(rename_all = "UPPERCASE")]
    Bar {
        bar_baz: i32,
        other: i32,
    },
}

#[test]
fn rename_all_fields_inner_mutation() {
    let mut r = RenameFields::Foo { foo_bar: 1, other: 0 };
    let mut ob = r.__observe();
    if let RenameFields::Foo { foo_bar, .. } = ob.untracked_mut() {
        *foo_bar = 2;
    }
    let Json(mutation) = ob.flush().unwrap();
    assert_eq!(mutation, Some(replace!(Foo.fooBar, json!(2))));
}

#[test]
fn variant_rename_all_inner_mutation() {
    let mut r = RenameFields::Bar { bar_baz: 1, other: 0 };
    let mut ob = r.__observe();
    if let RenameFields::Bar { bar_baz, .. } = ob.untracked_mut() {
        *bar_baz = 2;
    }
    let Json(mutation) = ob.flush().unwrap();
    assert_eq!(mutation, Some(replace!(Bar.BAR_BAZ, json!(2))));
}

#[derive(Serialize, Observe)]
#[muon(expose)]
enum Cached {
    A {
        #[serde(skip)]
        cache: String,
        n: u32,
    },
    B,
}

/// After the containing `Vec` reallocates, the `Pointer` of a skipped variant field must be
/// relocated onto the new element (and stay writable, which Miri checks).
#[test]
fn skipped_variant_field_after_relocate() {
    let mut v = vec![Cached::A {
        cache: String::new(),
        n: 0,
    }];
    let mut ob = v.__observe();
    let _ = &mut ob[0];
    for _ in 0..64 {
        ob.push(Cached::B);
    }
    match &mut ob[0].variant {
        CachedObserverVariant::A { cache, n } => {
            cache.push_str("xy");
            *n.tracked_mut() += 1;
        }
        _ => unreachable!(),
    }
    let Json(mutation) = ob.flush().unwrap();
    let mut expected = vec![json!({ "A": { "n": 1 } })];
    expected.extend(std::iter::repeat_n(json!("B"), 64));
    assert_eq!(mutation, Some(replace!(_, json!(expected))));
    drop(ob);
    let Cached::A { cache, n } = &v[0] else { unreachable!() };
    assert_eq!((cache.as_str(), *n), ("xy", 1));
}

#[derive(Serialize, Observe)]
#[muon(expose)]
enum OnlySkipped {
    A {
        #[serde(skip)]
        cache: String,
    },
    #[allow(dead_code)]
    B(u32),
}

#[derive(Serialize, Observe)]
struct BoxedOnlySkipped {
    inner: Box<OnlySkipped>,
}

/// Relocating onto a value that was never observed: the enum sits behind a `Box`, so the enclosing
/// observer only exposes the box slot, and the variant has no non-skipped sibling whose pointer
/// would expose the heap allocation. The skipped field's `Pointer` must then carry writable
/// provenance itself (checked by Miri). Reached through the stale-observer path of `swap_remove` +
/// `flush`.
#[test]
fn skipped_variant_field_relocated_onto_unobserved_box() {
    let new = || BoxedOnlySkipped {
        inner: Box::new(OnlySkipped::A { cache: String::new() }),
    };
    let mut v: Vec<BoxedOnlySkipped> = vec![];
    let mut ob = v.__observe();
    ob.push(new());
    ob.push(new());
    let _ = &mut ob[0].inner.variant;
    ob.swap_remove(0);
    let _ = ob.flush::<Json>();
    match &mut ob[0].inner.variant {
        OnlySkippedObserverVariant::A { cache } => cache.push_str("xy"),
        _ => unreachable!(),
    }
    drop(ob);
    let OnlySkipped::A { cache } = &*v[0].inner else {
        unreachable!()
    };
    assert_eq!(cache, "xy");
}
