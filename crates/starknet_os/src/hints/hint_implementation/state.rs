use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_types_core::felt::Felt;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids, Scope};

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

pub(crate) fn set_preimage_for_state_commitments<S: StateReader>(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, constants }: HintArgs<'_, S>,
) -> HintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    insert_value_from_var_name(
        Ids::InitialRoot.into(),
        os_input.contract_state_commitment_info.previous_root.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::FinalRoot.into(),
        os_input.contract_state_commitment_info.updated_root.0,
        vm,
        ids_data,
        ap_tracking,
    )?;

    // TODO(Dori): See if we can avoid the clone() here. Possible method: using `take()` to take
    //   ownership; we should, however, somehow invalidate the
    //   `os_input.contract_state_commitment_info.commitment_facts` field in this case (panic if
    //   accessed again after this line).
    exec_scopes.insert_value(
        Scope::Preimage.into(),
        os_input.contract_state_commitment_info.commitment_facts.clone(),
    );

    let merkle_height = Const::MerkleHeight.fetch(constants)?;
    let tree_height: Felt = os_input.contract_state_commitment_info.tree_height.into();
    assert_tree_height_eq_merkle_height(tree_height, *merkle_height)?;

    Ok(())
}

pub(crate) fn set_preimage_for_class_commitments<S: StateReader>(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, constants }: HintArgs<'_, S>,
) -> HintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    insert_value_from_var_name(
        Ids::InitialRoot.into(),
        os_input.contract_class_commitment_info.previous_root.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::FinalRoot.into(),
        os_input.contract_class_commitment_info.updated_root.0,
        vm,
        ids_data,
        ap_tracking,
    )?;

    // TODO(Dori): See if we can avoid the clone() here. Possible method: using `take()` to take
    //   ownership; we should, however, somehow invalidate the
    //   `os_input.contract_state_commitment_info.commitment_facts` field in this case (panic if
    //   accessed again after this line).
    exec_scopes.insert_value(
        Scope::Preimage.into(),
        os_input.contract_class_commitment_info.commitment_facts.clone(),
    );

    let merkle_height = Const::MerkleHeight.fetch(constants)?;
    let tree_height: Felt = os_input.contract_class_commitment_info.tree_height.into();
    assert_tree_height_eq_merkle_height(tree_height, *merkle_height)?;

    Ok(())
}

pub(crate) fn set_preimage_for_current_commitment_info<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn load_edge<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn load_bottom<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn decode_node<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn write_split_result<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}
