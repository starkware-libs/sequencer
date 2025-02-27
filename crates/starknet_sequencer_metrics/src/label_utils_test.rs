use std::cmp::Eq;
use std::collections::HashSet;
use std::hash::Hash;

use strum::VariantNames;
use strum_macros::EnumVariantNames;

#[allow(dead_code)]
#[derive(Debug, EnumVariantNames, Clone, Copy)]
enum Color {
    Red,
    Green,
    Blue,
}

#[allow(dead_code)]
#[derive(Debug, EnumVariantNames, Clone, Copy)]
enum Size {
    Small,
    Medium,
    Large,
}

generate_permutations!(("color", Color), ("size", Size),);

fn are_slices_equal<T: Hash + Eq + Clone>(a: &[T], b: &[T]) -> bool {
    a.len() == b.len()
        && a.iter().cloned().collect::<HashSet<_>>() == b.iter().cloned().collect::<HashSet<_>>()
}

#[test]
fn generate_permutations() {
    let expected_values: [[(&str, &str); 2]; 9] = [
        [("color", "Red"), ("size", "Small")],
        [("color", "Red"), ("size", "Medium")],
        [("color", "Red"), ("size", "Large")],
        [("color", "Green"), ("size", "Small")],
        [("color", "Green"), ("size", "Medium")],
        [("color", "Green"), ("size", "Large")],
        [("color", "Blue"), ("size", "Small")],
        [("color", "Blue"), ("size", "Medium")],
        [("color", "Blue"), ("size", "Large")],
    ];

    assert!(are_slices_equal(&COLOR_SIZE_PERMUTATIONS, &expected_values), "Mismatch");
}
