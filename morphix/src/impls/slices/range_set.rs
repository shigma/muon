use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::{Bound, Range};

#[derive(Clone)]
#[repr(transparent)]
struct StartWrapper(Range<usize>);

impl PartialEq for StartWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.start == other.0.start
    }
}

impl Eq for StartWrapper {}

impl Ord for StartWrapper {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.start.cmp(&other.0.start)
    }
}

impl PartialOrd for StartWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Borrow<EndWrapper> for StartWrapper {
    fn borrow(&self) -> &EndWrapper {
        // SAFETY: both types are #[repr(transparent)] over Range<usize>
        unsafe { &*(self as *const StartWrapper as *const EndWrapper) }
    }
}

#[repr(transparent)]
struct EndWrapper(Range<usize>);

impl PartialEq for EndWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.end == other.0.end
    }
}

impl Eq for EndWrapper {}

impl Ord for EndWrapper {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.end.cmp(&other.0.end)
    }
}

impl PartialOrd for EndWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A sorted, non-overlapping set of `Range<usize>` intervals with automatic merging.
///
/// Backed by a `BTreeSet` with a `Borrow`-based trick that enables O(log n) lookups
/// by either range start or range end.
pub(crate) struct RangeSet {
    set: BTreeSet<StartWrapper>,
}

impl RangeSet {
    pub fn new() -> Self {
        Self { set: BTreeSet::new() }
    }

    pub fn clear(&mut self) {
        self.set.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Range<usize>> {
        self.set.iter().map(|e| &e.0)
    }

    pub fn insert(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let mut start = range.start;
        let mut end = range.end;

        // Step 1: Check left neighbor (last entry with start <= range.start).
        // It touches [start, end) if its end >= start.
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

        // Step 2: Consume all entries with start > (merged) start and start <= (merged) end.
        // These all touch since their start <= end and their end > start.
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

    pub fn remove(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }

        // Handle left overlapping entry (last entry with start <= range.start whose end > range.start).
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

        // Handle entries fully or partially within (range.start, range.end).
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
    ///
    /// Uses the `Borrow<EndWrapper>` trick for O(log n) initial seek (skipping entries
    /// with end <= query.start). The forward direction stops at start >= query.end;
    /// the backward direction skips trailing entries with start >= query.end.
    pub fn overlapping<'a>(&'a self, range: &Range<usize>) -> Overlapping<'a> {
        let sliver = EndWrapper(range.start..range.start);
        Overlapping {
            iter: self
                .set
                .range::<EndWrapper, _>((Bound::Excluded(&sliver), Bound::Unbounded)),
            query_end: range.end,
        }
    }

    /// Returns an iterator over gaps (uncovered sub-ranges) within the query range.
    pub fn gaps<'a>(&'a self, range: &Range<usize>) -> Gaps<'a> {
        let sliver = EndWrapper(range.start..range.start);
        Gaps {
            iter: self
                .set
                .range::<EndWrapper, _>((Bound::Excluded(&sliver), Bound::Unbounded)),
            cursor: range.start,
            query_end: range.end,
        }
    }
}

pub(crate) struct Overlapping<'a> {
    iter: std::collections::btree_set::Range<'a, StartWrapper>,
    query_end: usize,
}

impl<'a> Iterator for Overlapping<'a> {
    type Item = &'a Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.iter.next()?;
        (entry.0.start < self.query_end).then_some(&entry.0)
    }
}

impl DoubleEndedIterator for Overlapping<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.iter.next_back()?;
            if entry.0.start < self.query_end {
                return Some(&entry.0);
            }
        }
    }
}

pub(crate) struct Gaps<'a> {
    iter: std::collections::btree_set::Range<'a, StartWrapper>,
    cursor: usize,
    query_end: usize,
}

impl Iterator for Gaps<'_> {
    type Item = Range<usize>;

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
