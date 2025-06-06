use rstest::fixture;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::{class_hash, contract_address};

use crate::concurrency::versioned_state::{ThreadSafeVersionedState, VersionedState};
use crate::context::BlockContext;
use crate::execution::call_info::CallInfo;
use crate::state::cached_state::{CachedState, TransactionalState};
use crate::state::state_api::StateReader;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::transactions::ExecutableTransaction;

// Public Consts.

pub const DEFAULT_CHUNK_SIZE: usize = 64;

// Fixtures.

#[fixture]
pub fn contract_address() -> ContractAddress {
    contract_address!("0x18031991")
}

#[fixture]
pub fn class_hash() -> ClassHash {
    class_hash!(27_u8)
}

// Concurrency constructors.

// TODO(meshi, 01/06/2024): Consider making this a macro.
pub fn safe_versioned_state_for_testing(
    block_state: CachedState<DictStateReader>,
) -> ThreadSafeVersionedState<CachedState<DictStateReader>> {
    ThreadSafeVersionedState::new(VersionedState::new(block_state))
}

// Utils.

// Note: this function does not mutate the state.
pub fn create_fee_transfer_call_info<S: StateReader>(
    state: &mut CachedState<S>,
    account_tx: &AccountTransaction,
    concurrency_mode: bool,
) -> CallInfo {
    let block_context = BlockContext::create_for_account_testing();
    let mut transactional_state = TransactionalState::create_transactional(state);
    let execution_info =
        account_tx.execute_raw(&mut transactional_state, &block_context, concurrency_mode).unwrap();

    let execution_info = execution_info.fee_transfer_call_info.unwrap();
    transactional_state.abort();
    execution_info
}
