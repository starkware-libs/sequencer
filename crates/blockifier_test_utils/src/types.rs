/// Conversion from usize to u64. May fail on architectures with over 64 bits
/// of address space.
pub fn u64_from_usize(val: usize) -> u64 {
    val.try_into().expect("Conversion from usize to u64 should not fail.")
}
