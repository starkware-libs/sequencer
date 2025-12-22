use std::collections::{BTreeSet, HashMap};

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_api::block_hash::block_hash_calculator::gas_prices_to_hash;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hint_processor::state_update_pointers::StateUpdatePointers;
use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::output::load_public_keys_into_memory;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::insert_values_to_fields;

pub(crate) fn initialize_class_hashes<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { exec_scopes, .. }: HintArgs<'_>,
) -> OsHintResult {
    let class_hash_to_compiled_class_hash: HashMap<MaybeRelocatable, MaybeRelocatable> =
        hint_processor
            .get_current_execution_helper()?
            .cached_state
            .writes_compiled_class_hashes()
            .into_iter()
            .map(|(class_hash, compiled_class_hash)| {
                (class_hash.0.into(), compiled_class_hash.0.into())
            })
            .collect();
    exec_scopes.insert_value(Scope::InitialDict.into(), class_hash_to_compiled_class_hash);
    Ok(())
}

pub(crate) fn initialize_state_changes<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { exec_scopes, vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let cached_state = &hint_processor.get_current_execution_helper()?.cached_state;
    let writes_accessed_addresses: BTreeSet<_> =
        cached_state.writes_contract_addresses().into_iter().collect();
    let mut initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = HashMap::new();

    for contract_address in writes_accessed_addresses {
        let nonce = cached_state.get_nonce_at(contract_address)?;
        let class_hash = cached_state.get_class_hash_at(contract_address)?;
        let storage_ptr = vm.add_memory_segment();
        let state_entry_base = vm.add_memory_segment();
        insert_values_to_fields(
            state_entry_base,
            CairoStruct::StateEntry,
            vm,
            &[
                ("class_hash", class_hash.0.into()),
                ("storage_ptr", storage_ptr.into()),
                ("nonce", nonce.0.into()),
            ],
            hint_processor.program,
        )?;
        initial_dict.insert((*contract_address.0.key()).into(), state_entry_base.into());
    }
    exec_scopes.insert_value(Scope::InitialDict.into(), initial_dict);
    Ok(())
}

pub(crate) fn write_full_output_to_memory<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let full_output = Felt::from(hint_processor.os_hints_config.full_output);
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::WriteFullOutputToMemory), full_output)
}

pub(crate) fn configure_kzg_manager<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.serialize_data_availability_create_pages = true;
    Ok(())
}

// Checks that the calculated block hash is consistent with the expected block hash (sanity check).
pub(crate) fn check_block_hash_consistency<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    let calculated_block_hash = get_integer_from_var_name("block_hash", vm, ids_data, ap_tracking)?;

    if calculated_block_hash != os_input.new_block_hash.0 {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Calculated block hash {} does not match expected block hash {}",
                calculated_block_hash, os_input.new_block_hash.0
            ),
        });
    }

    Ok(())
}

pub(crate) fn starknet_os_input(HintArgs { .. }: HintArgs<'_>) -> OsHintResult {
    // Nothing to do here; OS input already available on the hint processor.
    Ok(())
}

pub(crate) fn init_state_update_pointer<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.state_update_pointers = Some(StateUpdatePointers::new(vm));
    Ok(())
}

pub(crate) fn get_n_blocks<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let n_blocks = hint_processor.n_blocks();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::GetBlocksNumber), n_blocks)
}

pub(crate) fn get_n_class_hashes_to_migrate<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let n_classes =
        hint_processor.get_current_execution_helper()?.os_block_input.class_hashes_to_migrate.len();
    insert_value_from_var_name(
        Ids::NClassesToMigrate.into(),
        n_classes,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}
pub(crate) fn log_remaining_blocks<S: StateReader>(
    hint_processor: &SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let n_blocks = get_integer_from_var_name(Ids::NBlocks.into(), vm, ids_data, ap_tracking)?;
    match hint_processor.get_current_execution_helper() {
        Ok(current_helper) => {
            let block_number = current_helper.os_block_input.block_info.block_number;
            log::info!(
                "execute_blocks: finished executing block {block_number}, {n_blocks} blocks \
                 remaining."
            );
        }
        Err(_) => {
            // First iteration - no previous block finished yet.
            log::info!("execute_blocks: {n_blocks} blocks remaining.");
        }
    }
    Ok(())
}

pub(crate) fn create_block_additional_hints<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.execution_helpers_manager.increment_current_helper_index();
    Ok(())
}

pub(crate) fn get_public_keys<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let public_keys = hint_processor.os_hints_config.public_keys.clone();
    load_public_keys_into_memory(vm, ids_data, ap_tracking, public_keys)?;
    Ok(())
}

pub(crate) fn get_block_hashes<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    let block_info = &os_input.block_info;
    let gas_prices = &block_info.gas_prices;
    let commitments = &os_input.block_hash_commitments;

    insert_value_from_var_name(
        "parent_hash",
        os_input.prev_block_hash.0,
        vm,
        ids_data,
        ap_tracking,
    )?;

    let header_commitments_ptr = vm.add_memory_segment();
    insert_values_to_fields(
        header_commitments_ptr,
        CairoStruct::BlockHeaderCommitments,
        vm,
        &[
            ("transaction_commitment", commitments.transaction_commitment.0.into()),
            ("event_commitment", commitments.event_commitment.0.into()),
            ("receipt_commitment", commitments.receipt_commitment.0.into()),
            ("state_diff_commitment", commitments.state_diff_commitment.0.0.into()),
            ("concatenated_counts", commitments.concatenated_counts.into()),
        ],
        hint_processor.program,
    )?;
    insert_value_from_var_name(
        "header_commitments",
        header_commitments_ptr,
        vm,
        ids_data,
        ap_tracking,
    )?;

    let starknet_version_felt = Felt::try_from(&block_info.starknet_version)?;
    insert_value_from_var_name(
        "starknet_version",
        starknet_version_felt,
        vm,
        ids_data,
        ap_tracking,
    )?;

    let [gas_prices_hash]: [Felt; 1] = gas_prices_to_hash(
        &gas_prices.l1_gas_price_per_token(),
        &gas_prices.l1_data_gas_price_per_token(),
        &gas_prices.l2_gas_price_per_token(),
        &block_info.starknet_version.clone().try_into()?,
    )
    .try_into()
    .map_err(|_| {
        OsHintError::UnsupportedStarknetVersionForBlockHash(block_info.starknet_version.clone())
    })?;

    insert_value_from_var_name("gas_prices_hash", gas_prices_hash, vm, ids_data, ap_tracking)?;

    Ok(())
}
