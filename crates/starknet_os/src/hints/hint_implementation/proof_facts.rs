use std::vec::IntoIter;

use cairo_program_runner_lib::SIMPLE_BOOTLOADER_INPUT;
use starknet_types_core::felt::Felt;

use crate::hint_processor::aggregator_hint_processor::AggregatorHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintContext;
use crate::hints::vars::{Ids, Scope};
use crate::proof_fact_fold::flatten_proof_facts_tree;

pub(crate) fn load_proof_facts_tree(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let packed_tree =
        hint_processor.input.proof_facts_tree.as_ref().ok_or(OsHintError::MissingProofFactsTree)?;
    // `GuessProofTreeNode` guesses the nodes one at a time, in this order.
    ctx.insert_into_scope(
        Scope::ProofTreeNodes,
        flatten_proof_facts_tree(packed_tree)?.into_iter(),
    );
    Ok(())
}

pub(crate) fn guess_proof_tree_node(mut ctx: HintContext<'_>) -> OsHintResult {
    let proof_tree_nodes: &mut IntoIter<Felt> =
        ctx.exec_scopes.get_mut_ref(Scope::ProofTreeNodes.into())?;
    let node = proof_tree_nodes.next().ok_or(OsHintError::ProofFactsTreeExhausted)?;
    ctx.insert_value(Ids::Node, node)?;
    Ok(())
}

pub(crate) fn load_circuit_verifier_task(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let verifier_task = hint_processor
        .input
        .proof_facts_verifier_task
        .as_ref()
        .ok_or(OsHintError::MissingProofFactsVerifierTask)?;
    // The simple bootloader that runs the verifier reads its input from this scope variable.
    ctx.exec_scopes.insert_value(SIMPLE_BOOTLOADER_INPUT, verifier_task.simple_bootloader_input()?);
    Ok(())
}
