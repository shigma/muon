use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::{Bound, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};
use std::slice::{GetDisjointMutError, SliceIndex};

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
            .range((Bound::Excluded(StartWrapper(start..start)), Bound::Included(StartWrapper(end..end))))
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
            .range((Bound::Excluded(StartWrapper(range.start..range.start)), Bound::Excluded(StartWrapper(range.end..range.end))))
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

    /// Returns an iterator over stored ranges that overlap the query range.
    ///
    /// Uses the `Borrow<EndWrapper>` trick for O(log n) initial seek by end value,
    /// then advances until start >= query.end.
    pub fn overlapping<'a>(&'a self, range: &Range<usize>) -> impl Iterator<Item = &'a Range<usize>> + 'a {
        let query_end = range.end;
        let sliver = EndWrapper(range.start..range.start);
        self.set
            .range::<EndWrapper, _>((Bound::Excluded(&sliver), Bound::Unbounded))
            .take_while(move |e| e.0.start < query_end)
            .map(|e| &e.0)
    }

    /// Returns a lazy iterator over gaps (uncovered sub-ranges) within the query range.
    pub fn gaps(&self, range: &Range<usize>) -> impl Iterator<Item = Range<usize>> + '_ {
        let query_end = range.end;
        let sliver = EndWrapper(range.start..range.start);
        let mut iter = self
            .set
            .range::<EndWrapper, _>((Bound::Excluded(&sliver), Bound::Unbounded));
        let mut cursor = range.start;
        std::iter::from_fn(move || {
            for entry in iter.by_ref() {
                if entry.0.start >= query_end {
                    break;
                }
                let gap_start = cursor;
                let gap_end = entry.0.start;
                cursor = entry.0.end;
                if gap_start < gap_end {
                    return Some(gap_start..gap_end);
                }
            }
            if cursor < query_end {
                let gap = cursor..query_end;
                cursor = query_end;
                Some(gap)
            } else {
                None
            }
        })
    }
}

pub trait SliceIndexImpl {
    type Output<T>: ?Sized;

    fn index<T>(self, slice: &[T]) -> &Self::Output<T>;

    fn index_mut<T>(self, slice: &mut [T]) -> &mut Self::Output<T>;

    fn to_range(&self, len: usize) -> Range<usize>;
}

impl SliceIndexImpl for usize {
    type Output<T> = T;

    fn index<T>(self, slice: &[T]) -> &Self::Output<T> {
        &slice[self]
    }

    fn index_mut<T>(self, slice: &mut [T]) -> &mut Self::Output<T> {
        &mut slice[self]
    }

    fn to_range(&self, _len: usize) -> Range<usize> {
        *self..*self + 1
    }
}

impl SliceIndexImpl for Range<usize> {
    type Output<T> = [T];

    fn index<T>(self, slice: &[T]) -> &Self::Output<T> {
        &slice[self]
    }

    fn index_mut<T>(self, slice: &mut [T]) -> &mut Self::Output<T> {
        &mut slice[self]
    }

    fn to_range(&self, _len: usize) -> Range<usize> {
        self.clone()
    }
}

impl SliceIndexImpl for RangeFrom<usize> {
    type Output<T> = [T];

    fn index<T>(self, slice: &[T]) -> &Self::Output<T> {
        &slice[self]
    }

    fn index_mut<T>(self, slice: &mut [T]) -> &mut Self::Output<T> {
        &mut slice[self]
    }

    fn to_range(&self, len: usize) -> Range<usize> {
        self.start..len
    }
}

impl SliceIndexImpl for RangeTo<usize> {
    type Output<T> = [T];

    fn index<T>(self, slice: &[T]) -> &Self::Output<T> {
        &slice[self]
    }

    fn index_mut<T>(self, slice: &mut [T]) -> &mut Self::Output<T> {
        &mut slice[self]
    }

    fn to_range(&self, _len: usize) -> Range<usize> {
        0..self.end
    }
}

impl SliceIndexImpl for RangeFull {
    type Output<T> = [T];

    fn index<T>(self, slice: &[T]) -> &Self::Output<T> {
        slice
    }

    fn index_mut<T>(self, slice: &mut [T]) -> &mut Self::Output<T> {
        slice
    }

    fn to_range(&self, len: usize) -> Range<usize> {
        0..len
    }
}

impl SliceIndexImpl for RangeInclusive<usize> {
    type Output<T> = [T];

    fn index<T>(self, slice: &[T]) -> &Self::Output<T> {
        &slice[self]
    }

    fn index_mut<T>(self, slice: &mut [T]) -> &mut Self::Output<T> {
        &mut slice[self]
    }

    fn to_range(&self, _len: usize) -> Range<usize> {
        *self.start()..*self.end() + 1
    }
}

impl SliceIndexImpl for RangeToInclusive<usize> {
    type Output<T> = [T];

    fn index<T>(self, slice: &[T]) -> &Self::Output<T> {
        &slice[self]
    }

    fn index_mut<T>(self, slice: &mut [T]) -> &mut Self::Output<T> {
        &mut slice[self]
    }

    fn to_range(&self, _len: usize) -> Range<usize> {
        0..self.end + 1
    }
}

pub trait GetDisjointMutIndexImpl<T>: SliceIndex<[T]> + Sized {
    fn get_disjoint_mut<const N: usize>(
        slice: &mut [T],
        indices: [Self; N],
    ) -> Result<[&mut Self::Output; N], GetDisjointMutError>;

    unsafe fn get_disjoint_unchecked_mut<const N: usize>(slice: &mut [T], indices: [Self; N])
    -> [&mut Self::Output; N];
}

impl<T> GetDisjointMutIndexImpl<T> for usize {
    fn get_disjoint_mut<const N: usize>(
        slice: &mut [T],
        indices: [Self; N],
    ) -> Result<[&mut Self::Output; N], GetDisjointMutError> {
        slice.get_disjoint_mut(indices)
    }

    unsafe fn get_disjoint_unchecked_mut<const N: usize>(
        slice: &mut [T],
        indices: [Self; N],
    ) -> [&mut Self::Output; N] {
        unsafe { slice.get_disjoint_unchecked_mut(indices) }
    }
}

impl<T> GetDisjointMutIndexImpl<T> for Range<usize> {
    fn get_disjoint_mut<const N: usize>(
        slice: &mut [T],
        indices: [Self; N],
    ) -> Result<[&mut Self::Output; N], GetDisjointMutError> {
        slice.get_disjoint_mut(indices)
    }

    unsafe fn get_disjoint_unchecked_mut<const N: usize>(
        slice: &mut [T],
        indices: [Self; N],
    ) -> [&mut Self::Output; N] {
        unsafe { slice.get_disjoint_unchecked_mut(indices) }
    }
}

impl<T> GetDisjointMutIndexImpl<T> for RangeInclusive<usize> {
    fn get_disjoint_mut<const N: usize>(
        slice: &mut [T],
        indices: [Self; N],
    ) -> Result<[&mut Self::Output; N], GetDisjointMutError> {
        slice.get_disjoint_mut(indices)
    }

    unsafe fn get_disjoint_unchecked_mut<const N: usize>(
        slice: &mut [T],
        indices: [Self; N],
    ) -> [&mut Self::Output; N] {
        unsafe { slice.get_disjoint_unchecked_mut(indices) }
    }
}
