use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_relocatable_from_var_name,
    insert_value_from_var_name,
};
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids, Scope};
use crate::io::os_input::CommitmentInfo;

#[derive(Copy, Clone)]
pub(crate) enum CommitmentType {
    Class,
    State,
}

fn verify_tree_height_eq_merkle_height(tree_height: Felt, merkle_height: Felt) -> OsHintResult {
    if tree_height != merkle_height {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Tree height ({tree_height}) does not match Merkle height ({merkle_height})."
            ),
        });
    }

    Ok(())
}

fn set_preimage_for_commitments<S: StateReader>(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, constants }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    let CommitmentInfo { previous_root, updated_root, commitment_facts, tree_height } =
        match hint_processor.commitment_type {
            CommitmentType::Class => &os_input.contract_class_commitment_info,
            CommitmentType::State => &os_input.contract_state_commitment_info,
        };
    insert_value_from_var_name(
        Ids::InitialRoot.into(),
        previous_root.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(Ids::FinalRoot.into(), updated_root.0, vm, ids_data, ap_tracking)?;

    // TODO(Dori): See if we can avoid the clone() here. Possible method: using `take()` to take
    //   ownership; we should, however, somehow invalidate the
    //   `os_input.contract_state_commitment_info.commitment_facts` field in this case (panic if
    //   accessed again after this line).
    exec_scopes.insert_value(Scope::Preimage.into(), commitment_facts.clone());

    let merkle_height = Const::MerkleHeight.fetch(constants)?;
    let tree_height: Felt = (*tree_height).into();
    verify_tree_height_eq_merkle_height(tree_height, *merkle_height)?;

    Ok(())
}

pub(crate) fn compute_commitments_on_finalized_state_with_aliases<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    // TODO(Nimrod): Try to avoid this clone.
    exec_scopes.insert_value(
        Scope::CommitmentInfoByAddress.into(),
        hint_processor
            .get_current_execution_helper()?
            .os_block_input
            .address_to_storage_commitment_info
            .clone(),
    );

    Ok(())
}

pub(crate) fn set_preimage_for_state_commitments<S: StateReader>(
    hint_args: HintArgs<'_, S>,
) -> OsHintResult {
    hint_args.hint_processor.commitment_type = CommitmentType::State;
    set_preimage_for_commitments(hint_args)
}

pub(crate) fn set_preimage_for_class_commitments<S: StateReader>(
    hint_args: HintArgs<'_, S>,
) -> OsHintResult {
    hint_args.hint_processor.commitment_type = CommitmentType::Class;
    set_preimage_for_commitments(hint_args)
}

pub(crate) fn set_preimage_for_current_commitment_info<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn load_edge<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn load_bottom<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn decode_node<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn guess_state_ptr<S: StateReader>(
    HintArgs { hint_processor, ids_data, ap_tracking, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let state_changes_start =
        if let Some(state_update_pointers) = &hint_processor.state_update_pointers {
            state_update_pointers.get_state_entries_ptr()
        } else {
            vm.add_memory_segment()
        };
    Ok(insert_value_from_var_name(
        Ids::FinalSquashedContractStateChangesStart.into(),
        state_changes_start,
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn update_state_ptr<S: StateReader>(
    HintArgs { hint_processor, ids_data, ap_tracking, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.state_update_pointers {
        let contract_state_changes_end = get_relocatable_from_var_name(
            Ids::FinalSquashedContractStateChangesEnd.into(),
            vm,
            ids_data,
            ap_tracking,
        )?;
        state_update_pointers.set_state_entries_ptr(contract_state_changes_end);
    }
    Ok(())
}

pub(crate) fn guess_classes_ptr<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let class_changes_start =
        if let Some(state_update_pointers) = &hint_processor.state_update_pointers {
            state_update_pointers.get_classes_ptr()
        } else {
            vm.add_memory_segment()
        };
    Ok(insert_value_from_var_name(
        Ids::SquashedDict.into(),
        class_changes_start,
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn update_classes_ptr<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.state_update_pointers {
        let classes_changes_end =
            get_relocatable_from_var_name(Ids::SquashedDictEnd.into(), vm, ids_data, ap_tracking)?;
        state_update_pointers.set_classes_ptr(classes_changes_end);
    }
    Ok(())
}
