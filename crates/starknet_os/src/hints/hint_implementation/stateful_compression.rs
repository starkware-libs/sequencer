use blockifier::state::state_api::StateReader;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn enter_scope_with_aliases<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn get_alias_entry_for_state_update<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn key_lt_min_alias_alloc_value<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn assert_key_big_enough_for_alias<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_from_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn write_next_alias_from_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_counter<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn initialize_alias_counter<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn update_alias_counter<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn contract_address_le_max_for_compression<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn compute_commitments_on_finalized_state_with_aliases<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}
