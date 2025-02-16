use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_types_core::felt::Felt;

use crate::hint_processor::constants::Constants;
use crate::hint_processor::ids::Ids;
use crate::hint_processor::scopes::ScopedVariables;
use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

fn assert_tree_height_eq_merkle_height(tree_height: Felt, merkle_height: Felt) -> HintResult {
    if tree_height != merkle_height {
        return Err(HintError::AssertionFailed(
            format!("Tree height ({}) does not match Merkle height", tree_height)
                .to_string()
                .into_boxed_str(),
        ));
    }

    Ok(())
}

pub(crate) fn set_preimage_for_state_commitments(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, constants }: HintArgs<
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
    >,
) -> HintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    insert_value_from_var_name(
        Ids::InitialRoot.into(),
        os_input.contract_state_commitment_info.previous_root,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::FinalRoot.into(),
        os_input.contract_state_commitment_info.updated_root,
        vm,
        ids_data,
        ap_tracking,
    )?;

    // TODO(Dori): can we avoid this clone?
    let preimage = os_input.contract_state_commitment_info.commitment_facts.clone();
    exec_scopes.insert_value(ScopedVariables::Preimage.into(), preimage);

    let merkle_height = Constants::MerkleHeight.fetch(constants)?;
    let tree_height: Felt = os_input.contract_state_commitment_info.tree_height.into();
    assert_tree_height_eq_merkle_height(tree_height, *merkle_height)?;

    Ok(())
}

pub(crate) fn set_preimage_for_class_commitments(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_preimage_for_current_commitment_info(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn load_edge(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn load_bottom(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn decode_node(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn write_split_result(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}
