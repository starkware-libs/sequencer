use cairo_vm::types::relocatable::MaybeRelocatable;
use leaf_proof_format::PackedNode;
use starknet_types_core::felt::Felt;

use crate::hint_processor::aggregator_hint_processor::{AggregatorHintProcessor, DataAvailability};
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::aggregator::utils::FullOsOutputsData;
use crate::hints::types::HintContext;
use crate::hints::vars::Ids;
use crate::io::os_output_types::TryFromOutputIter;
use crate::vm_utils::LoadIntoVmMemory;

pub(crate) fn allocate_segments_for_messages(mut ctx: HintContext<'_>) -> OsHintResult {
    let messages_to_l1_segment = ctx.vm.add_temporary_segment();
    let messages_to_l2_segment = ctx.vm.add_temporary_segment();
    // Unlike the message segments, the proof facts are copied to the output, not relocated to it,
    // so their segment must not be temporary.
    let tx_proof_facts_segment = ctx.vm.add_memory_segment();
    let initial_carried_outputs = ctx.vm.gen_arg(&vec![
        messages_to_l1_segment,
        messages_to_l2_segment,
        tx_proof_facts_segment,
    ])?;
    ctx.insert_value(Ids::InitialCarriedOutputs, initial_carried_outputs)?;
    Ok(())
}

pub(crate) fn disable_da_page_creation(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    // Note that `serialize_os_output` splits its output to memory pages (see
    // `OutputBuiltinRunner.add_page`).
    // Since this output is only used internally and will not be used in the final fact, we need to
    // disable page creation.
    hint_processor.serialize_data_availability_create_pages = false;
    Ok(())
}

pub(crate) fn get_os_output_for_inner_blocks(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let mut bootloader_iter = hint_processor
        .input
        .bootloader_output
        .take()
        .ok_or(OsHintError::AggregatorBootloaderInputAlreadyConsumed)?
        .into_iter();

    // Regardless of encryption mode: OS outputs that reach the aggregator are never
    // encrypted. No need to pass any decryption data here.
    let FullOsOutputsData { outputs, n_outputs, program_hash } =
        FullOsOutputsData::try_from_output_iter(&mut bootloader_iter, None)?;

    ctx.insert_value(Ids::OsProgramHash, program_hash)?;
    ctx.insert_value(Ids::NTasks, n_outputs)?;

    let os_output_ptr = ctx.get_ptr(Ids::OsOutputs)?;
    outputs.load_into_vm_memory(ctx.vm, os_output_ptr)?;
    Ok(())
}

pub(crate) fn get_aggregator_output(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    // This impl differs from the python one, as we don't need to support an input of
    // polynomial_coefficients_to_kzg_commitment function anymore.
    hint_processor.serialize_data_availability_create_pages = true;
    Ok(())
}

pub(crate) fn write_da_segment(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    if let DataAvailability::Blob(da_file_path) = hint_processor.input.da.clone() {
        let da_segment = hint_processor.get_da_segment();

        std::fs::write(da_file_path, serde_json::to_string(&da_segment)?)?;
    }
    Ok(())
}

pub(crate) fn get_use_kzg_da_and_full_output_from_input(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let use_kzg_da: Felt = match hint_processor.input.da {
        DataAvailability::Blob(_) => true,
        DataAvailability::CallData => false,
    }
    .into();
    let full_output: Felt = hint_processor.input.full_output.into();
    ctx.insert_value(Ids::UseKzgDa, use_kzg_da)?;
    ctx.insert_value(Ids::FullOutput, full_output)?;
    Ok(())
}

pub(crate) fn set_state_update_pointers_to_none<CHP: CommonHintProcessor>(
    hint_processor: &mut CHP,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    *hint_processor.get_mut_state_update_pointers() = None;
    Ok(())
}

pub(crate) fn get_chain_id_and_fee_token_address_from_input(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let chain_id: Felt = hint_processor.input.chain_id;
    let fee_token_address: Felt = hint_processor.input.fee_token_address;
    ctx.insert_value(Ids::ChainId, chain_id)?;
    ctx.insert_value(Ids::FeeTokenAddress, fee_token_address)?;
    Ok(())
}

pub(crate) fn guess_proof_facts_tree(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let packed_tree =
        hint_processor.input.proof_facts_tree.as_ref().ok_or(OsHintError::MissingProofFactsTree)?;
    let flat_tree = flatten_proof_facts_tree(packed_tree)?;
    ctx.insert_value(Ids::FlatTreeSize, Felt::from(flat_tree.len()))?;
    let flat_tree_ptr =
        ctx.vm.gen_arg(&flat_tree.into_iter().map(MaybeRelocatable::from).collect::<Vec<_>>())?;
    ctx.insert_value(Ids::FlatTree, flat_tree_ptr)?;
    Ok(())
}

/// Writes the tree's nodes the way the Cairo `write_proof_tree_root_entry` reads them: one entry
/// per node, FLAT_TREE_FOLD (1) for a multiverifier proof of two children and FLAT_TREE_LEAF (0)
/// for a transaction's proof, each fold followed by its left subtree, then its right subtree. The
/// proving side's single-leaf tree, whose root multiverifier verifies the leaf in both of its
/// slots, is written as the leaf.
fn flatten_proof_facts_tree(packed_tree: &PackedNode) -> Result<Vec<Felt>, OsHintError> {
    let tree = match packed_tree {
        PackedNode::Composite { subtasks, .. } => match subtasks.as_slice() {
            [leaf @ PackedNode::Composite { subtasks: leaf_subtasks, .. }]
                if matches!(leaf_subtasks.as_slice(), [PackedNode::Plain { .. }]) =>
            {
                leaf
            }
            _ => packed_tree,
        },
        PackedNode::Plain { .. } => packed_tree,
    };
    let mut flat_tree = Vec::new();
    write_flat_subtree(tree, &mut flat_tree)?;
    Ok(flat_tree)
}

fn write_flat_subtree(node: &PackedNode, flat_tree: &mut Vec<Felt>) -> Result<(), OsHintError> {
    let unsupported_node = || OsHintError::UnsupportedProofFactsTreeNode(format!("{node:?}"));
    let PackedNode::Composite { subtasks, .. } = node else {
        return Err(unsupported_node());
    };
    match subtasks.as_slice() {
        [PackedNode::Plain { .. }] => flat_tree.push(Felt::ZERO),
        [left, right] => {
            flat_tree.push(Felt::ONE);
            write_flat_subtree(left, flat_tree)?;
            write_flat_subtree(right, flat_tree)?;
        }
        _ => return Err(unsupported_node()),
    }
    Ok(())
}
