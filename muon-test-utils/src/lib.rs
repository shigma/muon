#![allow(rustdoc::broken_intra_doc_links)]

//! Test helper macros for constructing [`Mutation`](::muon::Mutation) values concisely.
//!
//! # Path Syntax
//!
//! - `_` — root (empty path)
//! - `foo` — string segment
//! - `0` — positive index
//! - `-1` — negative index
//! - `foo.0.-1.bar` — mixed segments separated by `.`
//!
//! # Macros
//!
//! - [`replace!(path, value)`](replace!)
//! - [`append!(path, value)`](append!) (feature `append`)
//! - [`truncate!(path, len)`](truncate!) (feature `truncate`)
//! - [`delete!(path)`](delete!) (feature `delete`)
//! - [`batch!(path, items...)`](batch!)

#[doc(hidden)]
#[macro_export]
macro_rules! __mutation_path {
    (@munch [$($segments:expr),*]) => {
        vec![$($segments),*]
    };
    (@munch [$($segments:expr),*] . - $n:literal $($rest:tt)*) => {
        $crate::__mutation_path!(@munch [$($segments,)* ::muon::PathSegment::Negative($n)] $($rest)*)
    };
    (@munch [$($segments:expr),*] . $name:ident $($rest:tt)*) => {
        $crate::__mutation_path!(@munch [$($segments,)* ::muon::PathSegment::from(stringify!($name))] $($rest)*)
    };
    (@munch [$($segments:expr),*] . $n:literal $($rest:tt)*) => {
        $crate::__mutation_path!(@munch [$($segments,)* ::muon::PathSegment::Positive($n)] $($rest)*)
    };
    (_) => {
        vec![]
    };
    (- $n:literal $($rest:tt)*) => {
        $crate::__mutation_path!(@munch [::muon::PathSegment::Negative($n)] $($rest)*)
    };
    ($name:ident $($rest:tt)*) => {
        $crate::__mutation_path!(@munch [::muon::PathSegment::from(stringify!($name))] $($rest)*)
    };
    ($n:literal $($rest:tt)*) => {
        $crate::__mutation_path!(@munch [::muon::PathSegment::Positive($n)] $($rest)*)
    };
}

#[macro_export]
macro_rules! replace {
    (@parse [$($path:tt)*], $value:expr) => {
        ::muon::Mutation {
            path: $crate::__mutation_path!($($path)*).into(),
            kind: ::muon::MutationKind::Replace($value),
        }
    };
    (@parse [$($path:tt)*] $next:tt $($rest:tt)*) => {
        $crate::replace!(@parse [$($path)* $next] $($rest)*)
    };
    ($($all:tt)*) => { $crate::replace!(@parse [] $($all)*) };
}

#[cfg(feature = "append")]
#[macro_export]
macro_rules! append {
    (@parse [$($path:tt)*], $value:expr) => {
        ::muon::Mutation {
            path: $crate::__mutation_path!($($path)*).into(),
            kind: ::muon::MutationKind::Append($value),
        }
    };
    (@parse [$($path:tt)*] $next:tt $($rest:tt)*) => {
        $crate::append!(@parse [$($path)* $next] $($rest)*)
    };
    ($($all:tt)*) => { $crate::append!(@parse [] $($all)*) };
}

#[cfg(feature = "truncate")]
#[macro_export]
macro_rules! truncate {
    (@parse [$($path:tt)*], $value:expr) => {
        ::muon::Mutation {
            path: $crate::__mutation_path!($($path)*).into(),
            kind: ::muon::MutationKind::Truncate($value),
        }
    };
    (@parse [$($path:tt)*] $next:tt $($rest:tt)*) => {
        $crate::truncate!(@parse [$($path)* $next] $($rest)*)
    };
    ($($all:tt)*) => { $crate::truncate!(@parse [] $($all)*) };
}

#[cfg(feature = "delete")]
#[macro_export]
macro_rules! delete {
    (@parse [$($path:tt)*]) => {
        ::muon::Mutation {
            path: $crate::__mutation_path!($($path)*).into(),
            kind: ::muon::MutationKind::Delete,
        }
    };
    (@parse [$($path:tt)*] $next:tt $($rest:tt)*) => {
        $crate::delete!(@parse [$($path)* $next] $($rest)*)
    };
    ($($all:tt)*) => { $crate::delete!(@parse [] $($all)*) };
}

#[macro_export]
macro_rules! batch {
    (@parse [$($path:tt)*], $($items:expr),* $(,)?) => {
        ::muon::Mutation {
            path: $crate::__mutation_path!($($path)*).into(),
            kind: ::muon::MutationKind::Batch(vec![$($items),*]),
        }
    };
    (@parse [$($path:tt)*] $next:tt $($rest:tt)*) => {
        $crate::batch!(@parse [$($path)* $next] $($rest)*)
    };
    ($($all:tt)*) => { $crate::batch!(@parse [] $($all)*) };
}
