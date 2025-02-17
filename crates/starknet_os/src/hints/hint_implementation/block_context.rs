use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintExtensionResult, OsHintResult};
use crate::hints::types::HintArgs;

// Hint implementations.

pub(crate) fn load_class_inner<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn bytecode_segment_structure<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn block_number<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_number = hint_processor.execution_helper.os_input.block_info.block_number;
    Ok(insert_value_into_ap(vm, Felt::from(block_number.0))?)
}

pub(crate) fn block_timestamp<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_timestamp = hint_processor.execution_helper.os_input.block_info.block_timestamp;
    Ok(insert_value_into_ap(vm, Felt::from(block_timestamp.0))?)
}

pub(crate) fn chain_id<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn fee_token_address<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn deprecated_fee_token_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn sequencer_address<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let address = hint_processor.execution_helper.os_input.block_info.sequencer_address;
    Ok(insert_value_into_ap(vm, address.0.key())?)
}

pub(crate) fn get_block_mapping<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn is_leaf<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn write_use_kzg_da_to_memory<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

// Hint extension implementations.

pub(crate) fn load_class<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintExtensionResult {
    todo!()
}
