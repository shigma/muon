use std::ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};
use std::slice::{GetDisjointMutError, SliceIndex};

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
