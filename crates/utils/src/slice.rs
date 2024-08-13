use std::ops::RangeBounds;

pub fn safe_slice<T, R>(slice: &[T], range: R) -> Result<&[T], &'static str>
where
    R: RangeBounds<usize>,
{
    let start = match range.start_bound() {
        std::ops::Bound::Included(&start) => start,
        std::ops::Bound::Excluded(&start) => start + 1,
        std::ops::Bound::Unbounded => 0,
    };

    let end = match range.end_bound() {
        std::ops::Bound::Included(&end) => end + 1,
        std::ops::Bound::Excluded(&end) => end,
        std::ops::Bound::Unbounded => slice.len(),
    };

    if start <= end && end <= slice.len() {
        Ok(&slice[start..end])
    } else {
        Err("Slice not possible")
    }
}
