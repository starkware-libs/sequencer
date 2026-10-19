use cairo_program_runner_lib::SIMPLE_BOOTLOADER_INPUT;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintContext;
use crate::hints::vars::Ids;
use crate::proof_fact_fold::flatten_proof_facts_tree;

pub(crate) fn guess_proof_facts_tree<CHP: CommonHintProcessor>(
    hint_processor: &CHP,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let packed_tree =
        hint_processor.get_proof_facts_tree().ok_or(OsHintError::MissingProofFactsTree)?;
    let flat_tree = flatten_proof_facts_tree(packed_tree)?;
    ctx.insert_value(Ids::FlatTreeSize, Felt::from(flat_tree.len()))?;
    let flat_tree_ptr =
        ctx.vm.gen_arg(&flat_tree.into_iter().map(MaybeRelocatable::from).collect::<Vec<_>>())?;
    ctx.insert_value(Ids::FlatTree, flat_tree_ptr)?;
    Ok(())
}

pub(crate) fn load_circuit_verifier_task<CHP: CommonHintProcessor>(
    hint_processor: &CHP,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let verifier_task = hint_processor
        .get_proof_facts_verifier_task()
        .ok_or(OsHintError::MissingProofFactsVerifierTask)?;
    // The simple bootloader that runs the verifier reads its input from this scope variable.
    ctx.exec_scopes.insert_value(SIMPLE_BOOTLOADER_INPUT, verifier_task.simple_bootloader_input()?);
    Ok(())
}
