use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub fn assign_bytecode_segments(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub fn assert_end_of_bytecode_segments(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub fn delete_memory_data(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    // TODO(Yoni): Assert that the address was not accessed before.
    todo!()
}

pub fn iter_current_segment_info(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub fn set_ap_to_segment_hash(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub fn validate_compiled_class_facts_post_execution(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
