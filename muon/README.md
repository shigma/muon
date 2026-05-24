# muon

[![Crates.io](https://img.shields.io/crates/v/muon.svg)](https://crates.io/crates/muon)
[![Documentation](https://docs.rs/muon/badge.svg)](https://docs.rs/muon)

A Rust library for observing and serializing mutations.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
muon = { version = "0.19", features = ["json"] }
```

## Basic Usage

```rs
use serde::Serialize;
use serde_json::json;
use muon::adapter::Json;
use muon::{Mutation, MutationKind, Observe, observe};

// 1. Define any data structure with `#[derive(Observe)]`.
#[derive(Serialize, PartialEq, Debug, Observe)]
struct Foo {
    pub bar: Bar,
    pub qux: String,
}

#[derive(Serialize, PartialEq, Debug, Observe)]
struct Bar {
    pub baz: i32,
}

let mut foo = Foo {
    bar: Bar { baz: 42 },
    qux: "hello".to_string(),
};

// 2. Use `observe!` to mutate data and track mutations.
let Json(mutation) = observe!(foo => {
    foo.bar.baz += 1;
    foo.qux.push(' ');
    foo.qux += "world";
}).unwrap();

// 3. Inspect the mutations.
assert_eq!(
    mutation,
    Some(Mutation {
        path: vec![].into(),
        kind: MutationKind::Batch(vec![
            Mutation {
                path: vec!["bar".into()].into(),
                kind: MutationKind::Replace(json!({"baz": 43})),
            },
            Mutation {
                path: vec!["qux".into()].into(),
                kind: MutationKind::Append(json!(" world")),
            },
        ]),
    }),
);

// 4. The original data structure is also mutated.
assert_eq!(
    foo,
    Foo {
        bar: Bar { baz: 43 },
        qux: "hello world".to_string(),
    },
);
```

## Mutation Types

Morphix recognizes three types of mutations:

### Replace

The most general mutation type, used for any mutation that replaces a value:

```rs
foo.a.b = 1;        // Replace at .a.b
foo.num *= 2;       // Replace at .num
foo.vec.clear();    // Replace at .vec
```

### Append

Optimized for appending to strings and vectors:

```rs
foo.a.b += "text";          // Append to .a.b
foo.a.b.push_str("text");   // Append to .a.b
foo.vec.push(1);            // Append to .vec
foo.vec.extend(iter);       // Append to .vec
```

### Truncate

Optimized for truncating strings and vectors:

```rs
foo.a.b.truncate(5);        // Truncate n-5 chars from .a.b
foo.vec.pop();              // Truncate 1 element from .vec
```

### Delete

Used for deleting values from maps or conditionally skipping mutations:

```rs
foo.map.remove("key");      // Delete at .map.key
// #[serde(skip_serializing_if = "Option::is_none")]
foo.value = None;           // Delete at .value
```

### Batch

Multiple mutations combined into a single operation.

## Observer Mechanism

This section describes the internal mechanism of muon's observer system. It is intended for contributors and advanced users who want to understand how mutation tracking works under the hood.

### How Observers Work

An observer is a wrapper type that implements `Deref` and `DerefMut` to the type it observes. This lets the observer intercept all `&mut self` method calls through Rust's auto-deref mechanism. For example, a `StringObserver` dereferences to `String`, so calling `.push_str("hello")` on the observer transparently reaches the underlying `String` while the observer tracks the mutation.

For specific methods like `String::push_str` and `Vec::push`, observers provide specialized implementations that record precise mutations (e.g., `Append`). For any `&mut self` method that does *not* have a specialized implementation, the call falls through to `DerefMut`, which triggers a conservative `Replace` mutation covering the entire value. This means observers are always correct — they never miss a mutation — but unimplemented methods produce coarser-grained output.

### The Dereference Chain

For simple types like `String` or `i32`, an observer can deref directly to the target. But for types that already implement `Deref` — such as `Vec<T>`, which dereferences to `[T]` — a straightforward approach breaks down. If type `A` dereferences to `B`, and we have corresponding observers `A'` and `B'`, where should `A'` deref to?

- If `A'` → `A` → `B`: mutations on `B` cannot be precisely tracked (no `B'` in the chain).
- If `A'` → `B'` → `B`: properties and methods on `A` become inaccessible (no `A` in the chain).

The solution is to introduce a `Pointer<A>` to break the chain:

```text
A' → B' → Pointer<A> → A → B
```

This allows tracking mutations on both `A` and `B`. The chain is split into two segments:

```text
Self ──[OuterDepth]──> Pointer<Head> ───> Head ──[InnerDepth]──> Target
        coinductive                               inductive
```

- **OuterDepth**: The number of *coinductive* dereferences from the observer to its internal `Pointer`. For most observers (e.g., `StringObserver`, `HashMapObserver`), this is 1. For composite observers like `VecObserver`, which wraps `SliceObserver`, it is 2. For `Pointer<T>` itself, it is 0.
- **InnerDepth**: The number of *inductive* dereferences from the `Head` (the type stored in the `Pointer`) to the final observed `Target`. For example, a `VecObserver` has `Head = Vec<T>` and `Target = [T]`, so `InnerDepth = 1` (one `Deref` step).

These depths are tracked at the type level using `Zero` and `Succ<N>`, enabling the compiler to verify the chain is well-formed.

#### Tail and Non-Tail Observers

Observers are classified by their `Deref` target:

- **Tail observers** deref directly to `Pointer<S>` (e.g., `StringObserver`, `SliceObserver`, `HashMapObserver`). They are the innermost observer layer in the chain, sitting right next to the `Pointer`.
- **Non-tail observers** deref to another observer (e.g., `VecObserver` derefs to `SliceObserver`). They form outer layers in the chain.

This distinction matters for mutation tracking, as described in the next section.

### Primitives of Mutation Tracking

When a mutable method is called on an observer, one of three things can happen, depending on how the method is implemented:

#### Fully tracked operations (`untracked_mut`)

Methods like `Vec::push` or `String::push_str` have explicit observer implementations that know exactly what mutation occurred. These methods use `untracked_mut()` to access the underlying value *without* triggering any invalidation, then update the observer's diff state manually (e.g., incrementing an `append_index`).

No invalidation is needed because the observer already knows the precise mutation.

```rs
// Simplified implementation of Vec::push on VecObserver
fn push(&mut self, value: T) {
    self.untracked_mut().push(value);
    // The append_index tracking handles the rest —
    // flush will emit an Append mutation.
}
```

#### Coarse-grained operations (`tracked_mut`)

Methods like `Vec::retain` or `String::insert` modify the value in ways the observer cannot express with a granular mutation kind. These methods use `tracked_mut()`, which:

1. Calls `invalidate` on the current observer, resetting its diff state.
2. Propagates invalidation to all sibling observers that sit between this observer and the `Pointer` (i.e., observers in the "outer" direction).
3. Returns a mutable reference to the value via `DerefMutUntracked`, bypassing all `DerefMut` hooks.

After invalidation, the next `flush` will produce a `Replace` mutation for the affected value.

```rs
// Simplified implementation of Vec::retain on VecObserver
fn retain<F: FnMut(&T) -> bool>(&mut self, f: F) {
    self.tracked_mut().retain(f);
    // The observer's state is now invalidated —
    // flush will emit a Replace mutation.
}
```

#### Unimplemented methods (fallback invalidation)

For any `&mut self` method that has no explicit observer implementation, the call falls through to Rust's `DerefMut`. On a **tail observer**, `DerefMut` triggers **fallback invalidation**: it calls `Pointer::invalidate`, which iterates all registered observer states and invalidates them. This ensures that *every* observer in the chain is aware that an uncontrolled mutation may have occurred.

On a **non-tail observer**, `DerefMut` is a no-op pass-through to the inner observer. The inner observer's `DerefMut` (or the tail observer's fallback invalidation) handles the actual invalidation.

Fallback invalidation is maximally conservative: it invalidates the entire chain, causing a full `Replace` on the next flush. This guarantees correctness — no mutation is ever lost — at the cost of granularity for unimplemented methods.

### The QuasiObserver Trait

The `QuasiObserver` trait formalizes the dereference chain and provides the three primitives above as methods:

```rs
trait QuasiObserver {
    type Head: ?Sized;
    type OuterDepth: Unsigned;
    type InnerDepth: Unsigned;

    fn invalidate(this: &mut Self);
    fn untracked_ref(&self) -> &Target { .. }
    fn untracked_mut(&mut self) -> &mut Target { .. }
    fn tracked_mut(&mut self) -> &mut Target { .. }
}
```

Each method traverses the chain differently:

- **`untracked_ref()`** performs a read-only traversal: coinductive deref to the `Pointer`, then `Deref` (no side effects), then inductive deref to the `Target`. Since reads do not mutate, no invalidation is needed.
- **`tracked_mut()`** first calls `invalidate` on `self`, then reaches the `Target` via `DerefMutUntracked` — a special trait that bypasses all `DerefMut` hooks by using `Pointer`'s interior mutability to obtain `&mut` access through an immutable coinductive traversal. Only the observer on which `tracked_mut()` is called (and observers between it and the `Pointer`) are invalidated; outer observers are unaffected.
- **`untracked_mut()`** uses the same `DerefMutUntracked` path as `tracked_mut()`, but skips the `invalidate` call entirely. The caller is responsible for updating the diff state.

#### Autoref-Based Specialization

The `observe!` macro needs to transform assignment and comparison expressions to work uniformly with both observers and plain values. This creates two problems:

- **Assignment**: Writing `observer.field = value` would replace the observer itself rather than assigning to the observed field. The macro transforms this to `*(&mut observer.field).tracked_mut() = value`.
- **Comparison**: Implementing both `Observer<T>: PartialEq<U>` and `Observer<T>: PartialEq<Observer<U>>` would conflict. The macro transforms `lhs == rhs` to `*(&lhs).untracked_ref() == *(&rhs).untracked_ref()`.

For these transformations to work, `tracked_mut` and `untracked_ref` must be callable on both observers and plain references. This is achieved through autoref-based specialization: `QuasiObserver` is implemented for `&T` and `&mut T` (where all methods reduce to identity), and Rust's method resolution naturally selects the observer implementation when called on an observer, or the reference implementation when called on a plain value. The name "quasi-observer" reflects this dual nature — plain references are not real observers, but they participate in the same interface.

## MSRV

The minimum supported Rust version of muon is **1.89.0**.

Some APIs require newer Rust versions and are gated with `#[rustversion::since(...)]`.

## Features

- `derive` (default): Enables the `derive(Observe)` and `observe!` macros

- Mutation Kinds:
  - `append` (default): Enables `Append` mutation kind
  - `delete` (default): Enables `Delete` mutation kind
  - `truncate` (default): Enables `Truncate` mutation kind

- Truncate Length Encoding (mutually exclusive):
  - (default): `Truncate` lengths for `str`/`Path` are byte counts
  - `utf8`: `Truncate` lengths for `str`/`Path` are UTF-8 character counts
  - `utf16`: `Truncate` lengths for `str`/`Path` are UTF-16 code unit counts

- Adapters:
  - `json`: Includes JSON serialization support via `serde_json`
  - `yaml`: Includes YAML serialization support via `serde_yaml_ng`

- Third party integrations:
  - `chrono`
  - `indexmap`
  - `url`
  - `uuid`
