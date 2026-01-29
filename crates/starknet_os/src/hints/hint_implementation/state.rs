use blockifier::state::state_api::StateReader;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::commitment_infos::CommitmentInfo;
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Const, Ids};

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
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let CommitmentInfo { previous_root, updated_root, tree_height, .. } =
        hint_processor.get_commitment_info()?;
    ctx.insert_value(Ids::InitialRoot, previous_root.0)?;
    ctx.insert_value(Ids::FinalRoot, updated_root.0)?;

    // No need to insert the preimage map into the scope, as we extract it directly
    // from the execution helper.

    let merkle_height = ctx.fetch_const(Const::MerkleHeight)?;
    let tree_height = Felt::from(*tree_height);
    verify_tree_height_eq_merkle_height(tree_height, *merkle_height)?;

    Ok(())
}

pub(crate) fn compute_commitments_on_finalized_state_with_aliases(
    _ctx: HintContext<'_>,
) -> OsHintResult {
    // Do nothing here and use `address_to_storage_commitment_info` directly from the execution
    // helper.
    Ok(())
}

pub(crate) fn set_preimage_for_state_commitments<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    hint_processor.commitment_type = CommitmentType::State;
    set_preimage_for_commitments(hint_processor, ctx)
}

pub(crate) fn set_preimage_for_class_commitments<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    hint_processor.commitment_type = CommitmentType::Class;
    set_preimage_for_commitments(hint_processor, ctx)
}

pub(crate) fn set_preimage_for_current_commitment_info<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let contract_address: ContractAddress = ctx.get_integer(Ids::ContractAddress)?.try_into()?;
    hint_processor.commitment_type = CommitmentType::Contract(contract_address);
    let commitment_info = hint_processor.get_commitment_info()?;
    ctx.insert_value(Ids::InitialContractStateRoot, commitment_info.previous_root.0)?;
    ctx.insert_value(Ids::FinalContractStateRoot, commitment_info.updated_root.0)?;

    let tree_height = Felt::from(commitment_info.tree_height.0);
    let merkle_height = ctx.fetch_const(Const::MerkleHeight)?;
    verify_tree_height_eq_merkle_height(tree_height, *merkle_height)?;

    // No need to insert the preimage map into the scope, as we extract it directly
    // from the execution helper.
    Ok(())
}

pub(crate) fn should_use_read_optimized_patricia_update<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    // TODO(Yoni): this hint is a placeholder for future optimizations without changing the program
    // hash.
    Ok(ctx.insert_value(Ids::ShouldUseReadOptimized, Felt::ONE)?)
}

pub(crate) fn guess_state_ptr<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let state_changes_start =
        if let Some(state_update_pointers) = &hint_processor.state_update_pointers {
            state_update_pointers.get_state_entries_ptr()
        } else {
            ctx.vm.add_memory_segment()
        };
    Ok(ctx.insert_value(Ids::FinalSquashedContractStateChangesStart, state_changes_start)?)
}

pub(crate) fn update_state_ptr<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.state_update_pointers {
        let contract_state_changes_end = ctx.get_ptr(Ids::FinalSquashedContractStateChangesEnd)?;
        state_update_pointers.set_state_entries_ptr(contract_state_changes_end);
    }
    Ok(())
}

pub(crate) fn guess_classes_ptr<CHP: CommonHintProcessor>(
    hint_processor: &mut CHP,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let class_changes_start =
        if let Some(state_update_pointers) = &hint_processor.get_mut_state_update_pointers() {
            state_update_pointers.get_classes_ptr()
        } else {
            ctx.vm.add_memory_segment()
        };
    Ok(ctx.insert_value(Ids::SquashedDict, class_changes_start)?)
}

pub(crate) fn update_classes_ptr<CHP: CommonHintProcessor>(
    hint_processor: &mut CHP,
    ctx: HintContext<'_>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.get_mut_state_update_pointers() {
        let classes_changes_end = ctx.get_ptr(Ids::SquashedDictEnd)?;
        state_update_pointers.set_classes_ptr(classes_changes_end);
    }
    Ok(())
}
