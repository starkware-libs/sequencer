use std::collections::{BTreeSet, HashMap};

use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_api::block_hash::block_hash_calculator::gas_prices_to_hash;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hint_processor::state_update_pointers::StateUpdatePointers;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::insert_values_to_fields;

pub(crate) fn initialize_class_hashes<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
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
    ctx.insert_into_scope(Scope::InitialDict, class_hash_to_compiled_class_hash);
    Ok(())
}

pub(crate) fn initialize_state_changes<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let cached_state = &hint_processor.get_current_execution_helper()?.cached_state;
    let writes_accessed_addresses: BTreeSet<_> =
        cached_state.writes_contract_addresses().into_iter().collect();
    let mut initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = HashMap::new();

    for contract_address in writes_accessed_addresses {
        let nonce = cached_state.get_nonce_at(contract_address)?;
        let class_hash = cached_state.get_class_hash_at(contract_address)?;
        let storage_ptr = ctx.vm.add_memory_segment();
        let state_entry_base = ctx.vm.add_memory_segment();
        insert_values_to_fields(
            state_entry_base,
            CairoStruct::StateEntry,
            ctx.vm,
            &[
                ("class_hash", class_hash.0.into()),
                ("storage_ptr", storage_ptr.into()),
                ("nonce", nonce.0.into()),
            ],
            ctx.program,
        )?;
        initial_dict.insert((*contract_address.0.key()).into(), state_entry_base.into());
    }
    ctx.insert_into_scope(Scope::InitialDict, initial_dict);
    Ok(())
}

pub(crate) fn write_use_kzg_da_and_full_output_to_memory<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let os_hints_config = &hint_processor.os_hints_config;
    let use_kzg_da = Felt::from(os_hints_config.use_kzg_da && !os_hints_config.full_output);
    let full_output = Felt::from(os_hints_config.full_output);
    ctx.insert_value(Ids::UseKzgDa, use_kzg_da)?;
    ctx.insert_value(Ids::FullOutput, full_output)?;
    Ok(())
}

pub(crate) fn configure_kzg_manager<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    hint_processor.serialize_data_availability_create_pages = true;
    Ok(())
}

// Checks that the calculated block hash is consistent with the expected block hash (sanity check).
pub(crate) fn check_block_hash_consistency<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    let calculated_block_hash = ctx.get_integer(Ids::BlockHash)?;

    if calculated_block_hash != os_input.new_block_hash.0 {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Calculated block hash {calculated_block_hash} does not match expected block hash \
                 {}",
                os_input.new_block_hash.0
            ),
        });
    }

    Ok(())
}

pub(crate) fn starknet_os_input(_ctx: HintContext<'_>) -> OsHintResult {
    // Nothing to do here; OS input already available on the hint processor.
    Ok(())
}

pub(crate) fn init_state_update_pointer<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    hint_processor.state_update_pointers = Some(StateUpdatePointers::new(ctx.vm));
    Ok(())
}

pub(crate) fn get_n_blocks<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let n_blocks = hint_processor.n_blocks();
    Ok(ctx.insert_value(Ids::NBlocks, n_blocks)?)
}

pub(crate) fn get_n_class_hashes_to_migrate<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let n_classes =
        hint_processor.get_current_execution_helper()?.os_block_input.class_hashes_to_migrate.len();
    ctx.insert_value(Ids::NClassesToMigrate, n_classes)?;
    Ok(())
}
pub(crate) fn log_remaining_blocks<S: StateReader>(
    hint_processor: &SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let n_blocks = ctx.get_integer(Ids::NBlocks)?;
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
    _ctx: HintContext<'_>,
) -> OsHintResult {
    hint_processor.execution_helpers_manager.increment_current_helper_index();
    Ok(())
}

pub(crate) fn get_block_hashes<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    let block_info = &os_input.block_info;
    let gas_prices = &block_info.gas_prices;
    let commitments = &os_input.block_hash_commitments;

    ctx.insert_value(Ids::PreviousBlockHash, os_input.prev_block_hash.0)?;

    let header_commitments_ptr = ctx.vm.add_memory_segment();
    ctx.insert_values_to_fields(
        header_commitments_ptr,
        CairoStruct::BlockHeaderCommitments,
        &[
            ("transaction_commitment", commitments.transaction_commitment.0.into()),
            ("event_commitment", commitments.event_commitment.0.into()),
            ("receipt_commitment", commitments.receipt_commitment.0.into()),
            ("state_diff_commitment", commitments.state_diff_commitment.0.0.into()),
            ("packed_lengths", commitments.concatenated_counts.into()),
        ],
    )?;
    ctx.insert_value(Ids::HeaderCommitments, header_commitments_ptr)?;

    let starknet_version_felt = Felt::try_from(&block_info.starknet_version)?;
    ctx.insert_value(Ids::StarknetVersion, starknet_version_felt)?;

    let [gas_prices_hash]: [Felt; 1] = gas_prices_to_hash(
        &gas_prices.l1_gas_price_per_token(),
        &gas_prices.l1_data_gas_price_per_token(),
        &gas_prices.l2_gas_price_per_token(),
        &block_info.starknet_version.try_into()?,
    )
    .try_into()
    .map_err(|_| {
        OsHintError::UnsupportedStarknetVersionForBlockHash(block_info.starknet_version)
    })?;

    ctx.insert_value(Ids::GasPricesHash, gas_prices_hash)?;

    Ok(())
}
