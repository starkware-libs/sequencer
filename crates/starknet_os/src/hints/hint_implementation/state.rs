use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::io::os_input::CommitmentInfo;

#[derive(Copy, Clone)]
pub(crate) enum CommitmentType {
    Class,
    State,
    Contract(ContractAddress),
}

impl CommitmentType {
    pub(crate) fn hash_builtin_struct(&self) -> CairoStruct {
        match self {
            Self::Class => CairoStruct::SpongeHashBuiltin,
            Self::State | Self::Contract(_) => CairoStruct::HashBuiltin,
        }
    }
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
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_>,
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

pub(crate) fn compute_commitments_on_finalized_state_with_aliases(
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    // Do nothing here and use `address_to_storage_commitment_info` directly from the execution
    // helper.
    Ok(())
}

pub(crate) fn set_preimage_for_state_commitments<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    hint_args: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.commitment_type = CommitmentType::State;
    set_preimage_for_commitments(hint_processor, hint_args)
}

pub(crate) fn set_preimage_for_class_commitments<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    hint_args: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.commitment_type = CommitmentType::Class;
    set_preimage_for_commitments(hint_processor, hint_args)
}

pub(crate) fn set_preimage_for_current_commitment_info<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, constants, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let contract_address: ContractAddress =
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?
            .try_into()?;
    hint_processor.commitment_type = CommitmentType::Contract(contract_address);
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

pub(crate) fn should_use_read_optimized_patricia_update<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    // TODO(Yoni): this hint is a placeholder for future optimizations without changing the program
    // hash.
    Ok(insert_value_into_ap(vm, Felt::ONE)?)
}

pub(crate) fn guess_state_ptr<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { ids_data, ap_tracking, vm, .. }: HintArgs<'_>,
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
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { ids_data, ap_tracking, vm, .. }: HintArgs<'_>,
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

pub(crate) fn guess_classes_ptr<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let class_changes_start =
        if let Some(state_update_pointers) = &hint_processor.get_mut_state_update_pointers() {
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

pub(crate) fn update_classes_ptr<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.get_mut_state_update_pointers() {
        let classes_changes_end =
            get_ptr_from_var_name(Ids::SquashedDictEnd.into(), vm, ids_data, ap_tracking)?;
        state_update_pointers.set_classes_ptr(classes_changes_end);
    }
    Ok(())
}
