/// Number of bits encoding each element (per bucket).
pub(crate) const N_BITS_PER_BUCKET: [usize; 6] = [252, 125, 83, 62, 31, 15];
/// Number of buckets, including the repeating values bucket.
pub(crate) const TOTAL_N_BUCKETS: usize = N_BITS_PER_BUCKET.len() + 1;
