use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use cairo_vm::types::relocatable::Relocatable;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids, Scope};
use crate::io::os_input::{CommitmentInfo, OsBlockInput};

#[derive(Clone)]
pub(crate) struct StateUpdatePointers {
    pub(crate) _contract_address_to_storage_ptr: Relocatable,
    pub(crate) _state_tree_pointer: Relocatable,
    pub(crate) _class_tree_pointer: Relocatable,
}

#[derive(Copy, Clone)]
enum CommitmentType {
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
    commitment_type: CommitmentType,
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_input: &OsBlockInput = exec_scopes.get(Scope::BlockInput.into())?;
    let CommitmentInfo { previous_root, updated_root, commitment_facts, tree_height } =
        match commitment_type {
            CommitmentType::Class => &block_input.contract_class_commitment_info,
            CommitmentType::State => &block_input.contract_state_commitment_info,
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

pub(crate) fn set_preimage_for_state_commitments<S: StateReader>(
    hint_args: HintArgs<'_, S>,
) -> OsHintResult {
    set_preimage_for_commitments(CommitmentType::State, hint_args)
}

pub(crate) fn set_preimage_for_class_commitments<S: StateReader>(
    hint_args: HintArgs<'_, S>,
) -> OsHintResult {
    set_preimage_for_commitments(CommitmentType::Class, hint_args)
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

pub(crate) fn write_split_result<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
