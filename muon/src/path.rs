use std::borrow::Cow;
use std::fmt::{Debug, Display};
use std::ops::{Deref, DerefMut};

/// A segment of a mutation path.
///
/// [`PathSegment`] represents a single step in navigating to a nested value:
/// - [`String`](PathSegment::String): Access an object / struct field by name
/// - [`Positive`](PathSegment::Positive): Access an array / vec element by index from the start
/// - [`Negative`](PathSegment::Negative): Access an array / vec element by index from the end
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathSegment {
    /// A string key for accessing object/struct fields.
    String(Cow<'static, str>),
    /// A positive index for accessing elements from the start (0-based).
    Positive(usize),
    /// A negative index for accessing elements from the end (1-based, where 1 is the last element).
    Negative(usize),
}

impl From<usize> for PathSegment {
    fn from(n: usize) -> Self {
        Self::Positive(n)
    }
}

impl From<&'static str> for PathSegment {
    fn from(s: &'static str) -> Self {
        Self::String(Cow::Borrowed(s))
    }
}

impl From<String> for PathSegment {
    fn from(s: String) -> Self {
        Self::String(Cow::Owned(s))
    }
}

impl From<Cow<'static, str>> for PathSegment {
    fn from(s: Cow<'static, str>) -> Self {
        Self::String(s)
    }
}

impl Display for PathSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathSegment::String(s) => write!(f, ".{s}"),
            PathSegment::Positive(n) => write!(f, "[{n}]"),
            PathSegment::Negative(n) => write!(f, "[-{n}]"),
        }
    }
}

/// A path to a nested value within a data structure.
///
/// [`Path`] is a sequence of [`PathSegment`]s that describes how to navigate from a root value to a
/// nested value. The const parameter `REV` controls the internal storage order:
/// - `Path<false>`: Segments stored in natural order (root to leaf)
/// - `Path<true>`: Segments stored in reverse order (leaf to root), optimized for efficient `push`
///   and `pop` operations during mutation collection
#[derive(Default, Clone, PartialEq, Eq)]
pub struct Path<const REV: bool>(Vec<PathSegment>);

impl<const REV: bool> Path<REV> {
    /// Creates a new empty path.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<const REV: bool> From<Vec<PathSegment>> for Path<REV> {
    fn from(mut segments: Vec<PathSegment>) -> Self {
        if REV {
            segments.reverse();
        }
        Self(segments)
    }
}

impl<const REV: bool> Display for Path<REV> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if REV {
            for segment in self.0.iter().rev() {
                write!(f, "{segment}")?;
            }
        } else {
            for segment in self.0.iter() {
                write!(f, "{segment}")?;
            }
        };
        Ok(())
    }
}

impl<const REV: bool> Debug for Path<REV> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Path").field(&self.to_string()).finish()
    }
}

impl<const REV: bool> Deref for Path<REV> {
    type Target = Vec<PathSegment>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const REV: bool> DerefMut for Path<REV> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const REV: bool> FromIterator<PathSegment> for Path<REV> {
    fn from_iter<T: IntoIterator<Item = PathSegment>>(iter: T) -> Self {
        let mut segments: Vec<PathSegment> = iter.into_iter().collect();
        if REV {
            segments.reverse();
        }
        Self(segments)
    }
}
