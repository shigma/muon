use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::{Bound, Range};

pub(crate) trait RangeIndex: Ord + Copy {}

impl RangeIndex for usize {}
impl RangeIndex for isize {}

#[derive(Clone)]
#[repr(transparent)]
struct StartWrapper<T>(Range<T>);

impl<T: Ord> PartialEq for StartWrapper<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.start == other.0.start
    }
}

impl<T: Ord> Eq for StartWrapper<T> {}

impl<T: Ord> Ord for StartWrapper<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.start.cmp(&other.0.start)
    }
}

impl<T: Ord> PartialOrd for StartWrapper<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord> Borrow<EndWrapper<T>> for StartWrapper<T> {
    fn borrow(&self) -> &EndWrapper<T> {
        // SAFETY: both types are #[repr(transparent)] over Range<T>
        unsafe { &*(self as *const StartWrapper<T> as *const EndWrapper<T>) }
    }
}

#[repr(transparent)]
struct EndWrapper<T>(Range<T>);

impl<T: Ord> PartialEq for EndWrapper<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.end == other.0.end
    }
}

impl<T: Ord> Eq for EndWrapper<T> {}

impl<T: Ord> Ord for EndWrapper<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.end.cmp(&other.0.end)
    }
}

impl<T: Ord> PartialOrd for EndWrapper<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A sorted, non-overlapping set of `Range<T>` intervals with automatic merging.
///
/// Backed by a `BTreeSet` with a `Borrow`-based trick that enables O(log n) lookups
/// by either range start or range end.
pub(crate) struct RangeSet<T: RangeIndex = usize> {
    set: BTreeSet<StartWrapper<T>>,
}

impl<T: RangeIndex> RangeSet<T> {
    pub fn new() -> Self {
        Self { set: BTreeSet::new() }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Range<T>> {
        self.set.iter().map(|e| &e.0)
    }

    pub fn insert(&mut self, range: Range<T>) {
        if range.start >= range.end {
            return;
        }
        let mut start = range.start;
        let mut end = range.end;

        if let Some(left) = self
            .set
            .range(..=StartWrapper(start..start))
            .next_back()
            .filter(|e| e.0.end >= start)
            .cloned()
        {
            start = start.min(left.0.start);
            end = end.max(left.0.end);
            self.set.remove(&left);
        }

        while let Some(right) = self
            .set
            .range((
                Bound::Excluded(StartWrapper(start..start)),
                Bound::Included(StartWrapper(end..end)),
            ))
            .next()
            .cloned()
        {
            end = end.max(right.0.end);
            self.set.remove(&right);
        }

        self.set.insert(StartWrapper(start..end));
    }

    pub fn remove(&mut self, range: Range<T>) {
        if range.start >= range.end {
            return;
        }

        if let Some(left) = self
            .set
            .range(..=StartWrapper(range.start..range.start))
            .next_back()
            .filter(|e| e.0.end > range.start)
            .cloned()
        {
            self.set.remove(&left);
            if left.0.start < range.start {
                self.set.insert(StartWrapper(left.0.start..range.start));
            }
            if left.0.end > range.end {
                self.set.insert(StartWrapper(range.end..left.0.end));
                return;
            }
        }

        while let Some(entry) = self
            .set
            .range((
                Bound::Excluded(StartWrapper(range.start..range.start)),
                Bound::Excluded(StartWrapper(range.end..range.end)),
            ))
            .next()
            .cloned()
        {
            self.set.remove(&entry);
            if entry.0.end > range.end {
                self.set.insert(StartWrapper(range.end..entry.0.end));
                break;
            }
        }
    }

    /// Returns a double-ended iterator over stored ranges that overlap the query range.
    pub fn overlapping<'a>(&'a self, range: &Range<T>) -> Overlapping<'a, T> {
        let sliver = EndWrapper(range.start..range.start);
        Overlapping {
            iter: self
                .set
                .range::<EndWrapper<T>, _>((Bound::Excluded(&sliver), Bound::Unbounded)),
            query_end: range.end,
        }
    }

    /// Returns an iterator over gaps (uncovered sub-ranges) within the query range.
    pub fn gaps<'a>(&'a self, range: &Range<T>) -> Gaps<'a, T> {
        let sliver = EndWrapper(range.start..range.start);
        Gaps {
            iter: self
                .set
                .range::<EndWrapper<T>, _>((Bound::Excluded(&sliver), Bound::Unbounded)),
            cursor: range.start,
            query_end: range.end,
        }
    }
}

pub(crate) struct Overlapping<'a, T: RangeIndex = usize> {
    iter: std::collections::btree_set::Range<'a, StartWrapper<T>>,
    query_end: T,
}

impl<'a, T: RangeIndex> Iterator for Overlapping<'a, T> {
    type Item = &'a Range<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.iter.next()?;
        (entry.0.start < self.query_end).then_some(&entry.0)
    }
}

impl<T: RangeIndex> DoubleEndedIterator for Overlapping<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.iter.next_back()?;
            if entry.0.start < self.query_end {
                return Some(&entry.0);
            }
        }
    }
}

pub(crate) struct Gaps<'a, T: RangeIndex = usize> {
    iter: std::collections::btree_set::Range<'a, StartWrapper<T>>,
    cursor: T,
    query_end: T,
}

impl<T: RangeIndex> Iterator for Gaps<'_, T> {
    type Item = Range<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iter.next() {
                Some(entry) if entry.0.start < self.query_end => {
                    let gap_start = self.cursor;
                    let gap_end = entry.0.start;
                    self.cursor = entry.0.end;
                    if gap_start < gap_end {
                        return Some(gap_start..gap_end);
                    }
                }
                _ => {
                    if self.cursor < self.query_end {
                        let gap = self.cursor..self.query_end;
                        self.cursor = self.query_end;
                        return Some(gap);
                    }
                    return None;
                }
            }
        }
    }
}
