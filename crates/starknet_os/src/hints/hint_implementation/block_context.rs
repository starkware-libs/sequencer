use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;

use crate::hints::error::{HintExtensionResult, HintResult};
use crate::hints::types::HintArgs;

// Hint implementations.

pub(crate) fn load_class_inner(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn bytecode_segment_structure(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn block_number(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn block_timestamp(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn chain_id(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn fee_token_address(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn deprecated_fee_token_address(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn sequencer_address(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    let address = hint_processor.execution_helper.block_info.sequencer_address;
    insert_value_into_ap(vm, address.0.key())
}

pub(crate) fn get_block_mapping(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn is_leaf(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn write_use_kzg_da_to_memory(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

// Hint extension implementations.

pub(crate) fn load_class(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintExtensionResult {
    todo!()
}
