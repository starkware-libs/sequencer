use std::collections::HashMap;
use std::sync::LazyLock;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintError;

#[cfg(test)]
#[path = "nondet_offsets_test.rs"]
pub mod test;

/// Hashmap in which the keys are all hints of the form `nondet %{ .. %}`, and the values are the
/// generated offsets. These offsets can change when the OS code is changed, so we keep track of
/// the values here are test for consistency with the hint string; the offset in the hint
/// implementation should be fetched from this map.
pub(crate) static NONDET_FP_OFFSETS: LazyLock<HashMap<AllHints, usize>> = LazyLock::new(|| {
    HashMap::from([
        (AllHints::OsHint(OsHint::OsInputTransactions), 12),
        (AllHints::OsHint(OsHint::ReadAliasFromKey), 0),
        (AllHints::OsHint(OsHint::SetFpPlus4ToTxNonce), 4),
        (AllHints::OsHint(OsHint::WriteFullOutputToMemory), 25),
        (AllHints::OsHint(OsHint::WriteUseKzgDaToMemory), 24),
    ])
});

pub(crate) fn fetch_offset(hint: AllHints) -> Result<usize, OsHintError> {
    Ok(*NONDET_FP_OFFSETS.get(&hint).ok_or(OsHintError::MissingOffsetForHint { hint })?)
}
