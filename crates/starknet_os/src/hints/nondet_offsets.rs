use std::collections::HashMap;
use std::sync::LazyLock;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintError;

#[cfg(test)]
#[path = "nondet_offsets_test.rs"]
pub mod test;

/// Hashmap in which the keys are all hints compiled from cairo code of the form
/// `local x = nondet %{ y %}`. The resulting hint string will be of the form
/// `memory[fp + O] = to_felt_or_relocatable(y)` for some offset `O` depending on the cairo code in
/// the respective function (locals before this line can effect the offset). We keep track of the
/// values here, and test for consistency with the hint string; the offset in the hint
/// implementation should be fetched from this map.
pub(crate) static NONDET_FP_OFFSETS: LazyLock<HashMap<AllHints, usize>> = LazyLock::new(|| {
    HashMap::from([
        (AllHints::OsHint(OsHint::OsInputTransactions), 12),
        (AllHints::OsHint(OsHint::ReadAliasFromKey), 0),
        (AllHints::OsHint(OsHint::SetFpPlus4ToTxNonce), 4),
        (AllHints::OsHint(OsHint::WriteFullOutputToMemory), 16),
        (AllHints::OsHint(OsHint::WriteUseKzgDaToMemory), 15),
    ])
});

pub(crate) fn fetch_offset(hint: AllHints) -> Result<usize, OsHintError> {
    Ok(*NONDET_FP_OFFSETS.get(&hint).ok_or(OsHintError::MissingOffsetForHint { hint })?)
}
