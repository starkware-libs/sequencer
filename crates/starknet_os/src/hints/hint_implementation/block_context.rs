use blockifier::state::state_api::StateReader;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::hint_implementation::execution::utils::set_state_entry;
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Const, Ids};

// Hint implementations.

pub(crate) fn guess_block_info<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let block_info = &hint_processor.get_current_execution_helper()?.os_block_input.block_info;
    let block_info_ptr = ctx.vm.add_memory_segment();
    ctx.insert_value(Ids::BlockInfo, block_info_ptr)?;
    ctx.insert_to_fields(
        block_info_ptr,
        CairoStruct::BlockInfo,
        &[
            ("block_number", Felt::from(block_info.block_number.0).into()),
            ("block_timestamp", Felt::from(block_info.block_timestamp.0).into()),
            ("sequencer_address", (**block_info.sequencer_address).into()),
        ],
    )?;
    Ok(())
}

pub(crate) fn chain_id_and_fee_token_address<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let chain_info = &hint_processor.os_hints_config.chain_info;
    ctx.insert_value(Ids::ChainId, Felt::try_from(&chain_info.chain_id)?)?;
    ctx.insert_value(Ids::FeeTokenAddress, **chain_info.strk_fee_token_address)?;
    Ok(())
}

pub(crate) fn get_block_hash_mapping(ctx: HintContext<'_>) -> OsHintResult {
    let block_hash_contract_address = *ctx.fetch_const(Const::BlockHashContractAddress)?;
    set_state_entry(
        &block_hash_contract_address,
        ctx.vm,
        ctx.exec_scopes,
        ctx.ids_data,
        ctx.ap_tracking,
    )
}
