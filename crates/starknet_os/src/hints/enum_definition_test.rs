use std::collections::HashSet;

use strum::IntoEnumIterator;

use super::{Hint, HintExtension};
use crate::hints::types::HintEnum;

#[test]
fn test_hint_strings_are_unique() {
    let hint_strings = Hint::iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let hint_extension_strings =
        HintExtension::iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let hint_strings_set: HashSet<&&str> = HashSet::from_iter(hint_strings.iter());
    let hint_extension_strings_set = HashSet::from_iter(hint_extension_strings.iter());
    assert_eq!(hint_strings.len(), hint_strings_set.len(), "Duplicate hint strings.");
    assert_eq!(
        hint_extension_strings.len(),
        hint_extension_strings_set.len(),
        "Duplicate hint extension strings."
    );
    let ambiguous_strings =
        hint_strings_set.intersection(&hint_extension_strings_set).collect::<Vec<_>>();
    assert!(ambiguous_strings.is_empty(), "Ambiguous hint strings: {ambiguous_strings:?}");
}
