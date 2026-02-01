use apollo_proc_macros::unique_u16;
use static_assertions::const_assert_ne;

const A: u16 = unique_u16!();
const B: u16 = unique_u16!();
const C: u16 = unique_u16!();

// Test compile-time uniqueness.
const_assert_ne!(A, B);
const_assert_ne!(A, C);
const_assert_ne!(B, C);
