use rstest::rstest;

use crate::register_magic_constants;

#[rstest]
fn test_unique_keys(#[values(1, 2)] a: u32, #[values(3, 4)] b: u32) {
    let mut magic = register_magic_constants!();
    // Dependent keys.
    magic.assert_eq(&format!("7_PLUS_A_{a}"), 7 + a);
    magic.assert_eq(&format!("1_PLUS_B_{b}"), 1 + b);
    magic.assert_eq(&format!("A_{a}_PLUS_B_{b}"), a + b);
    // Independent keys.
    magic.assert_eq("C", 3);
}
