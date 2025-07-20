use std::collections::HashMap;
use std::sync::LazyLock;

use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::vm_core::VirtualMachine;

use crate::hints::enum_definition::{AllHints, OsHint, StatelessHint};
use crate::hints::error::{OsHintError, OsHintResult};

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
        (AllHints::StatelessHint(StatelessHint::SegmentsAddTemp), 7),
        (AllHints::OsHint(OsHint::SetFpPlus4ToTxNonce), 4),
        (AllHints::OsHint(OsHint::GetBlocksNumber), 0),
        (AllHints::OsHint(OsHint::GetNClassHashesToMigrate), 6),
        (AllHints::OsHint(OsHint::TxAccountDeploymentDataLen), 4),
        (AllHints::OsHint(OsHint::WriteFullOutputToMemory), 21),
        (AllHints::OsHint(OsHint::WriteUseKzgDaToMemory), 20),
    ])
});

fn fetch_offset(hint: AllHints) -> Result<usize, OsHintError> {
    Ok(*NONDET_FP_OFFSETS.get(&hint).ok_or(OsHintError::MissingOffsetForHint { hint })?)
}

pub(crate) fn insert_nondet_hint_value<T: Into<MaybeRelocatable>>(
    vm: &mut VirtualMachine,
    hint: AllHints,
    value: T,
) -> OsHintResult {
    let offset = fetch_offset(hint)?;
    Ok(vm.insert_value((vm.get_fp() + offset)?, value)?)
}
