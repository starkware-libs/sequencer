use std::collections::HashSet;

use strum::IntoEnumIterator;

use super::{Hint, HintExtension};
use crate::hints::types::HintEnum;

#[test]
fn test_hint_strings_are_unique() {
    let hint_strings = Hint::iter().map(|hint| hint.to_str()).collect::<HashSet<_>>();
    let hint_extension_strings =
        HintExtension::iter().map(|hint| hint.to_str()).collect::<HashSet<_>>();
    let ambiguous_strings = hint_strings.intersection(&hint_extension_strings).collect::<Vec<_>>();
    assert!(ambiguous_strings.is_empty(), "Ambiguous hint strings: {ambiguous_strings:?}");
}
