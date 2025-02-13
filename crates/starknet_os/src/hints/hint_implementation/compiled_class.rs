use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn assign_bytecode_segments(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn assert_end_of_bytecode_segments(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn delete_memory_data(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    // TODO(Yoni): Assert that the address was not accessed before.
    todo!()
}

pub(crate) fn iter_current_segment_info(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_ap_to_segment_hash(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn validate_compiled_class_facts_post_execution(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
