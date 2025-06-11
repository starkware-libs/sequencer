use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::io::os_input::CommitmentInfo;

#[derive(Copy, Clone)]
pub(crate) enum CommitmentType {
    Class,
    State,
}

impl CommitmentType {
    pub(crate) fn hash_builtin_struct(&self) -> CairoStruct {
        match self {
            Self::State => CairoStruct::HashBuiltin,
            Self::Class => CairoStruct::SpongeHashBuiltin,
        }
    }
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
fn set_preimage_for_commitments<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let CommitmentInfo { previous_root, updated_root, tree_height, .. } =
        hint_processor.get_commitment_info()?;
    insert_value_from_var_name(
        Ids::InitialRoot.into(),
        previous_root.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(Ids::FinalRoot.into(), updated_root.0, vm, ids_data, ap_tracking)?;

    // No need to insert the preimage map into the scope, as we extract it directly
    // from the execution helper.

    let merkle_height = Const::MerkleHeight.fetch(constants)?;
    let tree_height = Felt::from(*tree_height);
    verify_tree_height_eq_merkle_height(tree_height, *merkle_height)?;

    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn compute_commitments_on_finalized_state_with_aliases<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    // Do nothing here and use `address_to_storage_commitment_info` directly from the execution
    // helper.
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_preimage_for_state_commitments<S: StateReader>(
    hint_args: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_args.hint_processor.commitment_type = CommitmentType::State;
    hint_args.hint_processor.contract_address = None;
    set_preimage_for_commitments(hint_args)
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_preimage_for_class_commitments<S: StateReader>(
    hint_args: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_args.hint_processor.commitment_type = CommitmentType::Class;
    hint_args.hint_processor.contract_address = None;
    set_preimage_for_commitments(hint_args)
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_preimage_for_current_commitment_info<S: StateReader>(
    HintArgs { vm, constants, ids_data, ap_tracking, hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_processor.commitment_type = CommitmentType::State;
    let contract_address: ContractAddress =
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?
            .try_into()?;
    hint_processor.contract_address = Some(contract_address);
    let commitment_info = hint_processor.get_commitment_info()?;
    insert_value_from_var_name(
        Ids::InitialContractStateRoot.into(),
        commitment_info.previous_root.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::FinalContractStateRoot.into(),
        commitment_info.updated_root.0,
        vm,
        ids_data,
        ap_tracking,
    )?;

    let tree_height = Felt::from(commitment_info.tree_height.0);
    let merkle_height = Const::MerkleHeight.fetch(constants)?;
    verify_tree_height_eq_merkle_height(tree_height, *merkle_height)?;

    // No need to insert the preimage map into the scope, as we extract it directly
    // from the execution helper.
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn guess_state_ptr<S: StateReader>(
    HintArgs { hint_processor, ids_data, ap_tracking, vm, .. }: HintArgs<'_, '_, S>,
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

#[allow(clippy::result_large_err)]
pub(crate) fn update_state_ptr<S: StateReader>(
    HintArgs { hint_processor, ids_data, ap_tracking, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.state_update_pointers {
        let contract_state_changes_end = get_ptr_from_var_name(
            Ids::FinalSquashedContractStateChangesEnd.into(),
            vm,
            ids_data,
            ap_tracking,
        )?;
        state_update_pointers.set_state_entries_ptr(contract_state_changes_end);
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn guess_classes_ptr<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
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

#[allow(clippy::result_large_err)]
pub(crate) fn update_classes_ptr<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.state_update_pointers {
        let classes_changes_end =
            get_ptr_from_var_name(Ids::SquashedDictEnd.into(), vm, ids_data, ap_tracking)?;
        state_update_pointers.set_classes_ptr(classes_changes_end);
    }
    Ok(())
}
