use std::collections::HashMap;

use blockifier::execution::call_info::CallInfo;
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use shared_execution_objects::central_objects::{
    CallInfoIndex, CentralTransactionExecutionInfo
};
use starknet_api::executable_transaction::TransactionType;
use starknet_api::transaction::fields::Fee;

use crate::errors::StarknetOsError;
use crate::hint_processor::os_logger::OsLogger;
use crate::io::os_input::{CachedStateInput, OsBlockInput};

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    pub(crate) cached_state: CachedState<S>,
    pub(crate) os_block_input: OsBlockInput,
    pub(crate) os_logger: OsLogger,
    tx_execution_info_iter: TxExecutionInfoIter,
    call_info_index: CallInfoIndex,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(
        os_block_input: OsBlockInput,
        state_reader: S,
        state_input: CachedStateInput,
        debug_mode: bool,
    ) -> Result<Self, StarknetOsError> {
        Ok(Self {
            cached_state: Self::initialize_cached_state(state_reader, state_input)?,
            os_block_input,
            os_logger: OsLogger::new(debug_mode),
            tx_execution_info_iter: TxExecutionInfoIter::new(),
            call_info_index: CallInfoIndex::empty(),
        })
    }

    fn get_tx_execution_info(&self) -> Option<&CentralTransactionExecutionInfo> {
        self.tx_execution_info_iter.get(&self.os_block_input.tx_execution_infos)
    }

    pub(crate) fn next_tx_execution_infos(
        &mut self,
        tx_type: TransactionType,
    ) -> Option<TransactionExecutionInfoForExecutionHelper> {
        self.tx_execution_info_iter.increment();
        let tx_execution_info = self.get_tx_execution_info()?;
        let result = Some(tx_execution_info.into());
        self.call_info_index = CallInfoIndex::new(tx_execution_info, tx_type);
        result
    }

    #[allow(dead_code)]
    fn current_call_info(&self) -> Option<&CallInfo> {
        self.call_info_index.current_call_info(self.get_tx_execution_info()?)
    }

    #[allow(dead_code)]
    pub(crate) fn increment_call_info(&mut self) -> Option<()> {
        let tx_execution_info = self.get_tx_execution_info()?;
        let next_level_length =
            self.call_info_index.current_call_info(tx_execution_info)?.inner_calls.len();
        self.call_info_index.increment_call_info(next_level_length);
        Some(())
    }

    fn initialize_cached_state(
        state_reader: S,
        state_input: CachedStateInput,
    ) -> Result<CachedState<S>, StarknetOsError> {
        let mut empty_cached_state = CachedState::new(state_reader);
        let mut state_maps = StateMaps::default();

        // Insert storage.
        for (contract_address, storage) in state_input.storage.into_iter() {
            for (key, value) in storage.into_iter() {
                state_maps.storage.insert((contract_address, key), value);
            }
        }
        // Insert nonces.
        state_maps.nonces = state_input.address_to_nonce;

        // Insert class hashes.
        state_maps.class_hashes = state_input.address_to_class_hash;

        // Insert compiled class hashes.
        state_maps.compiled_class_hashes = state_input.class_hash_to_compiled_class_hash;

        // Update the cached state.
        empty_cached_state.update_cache(&state_maps, HashMap::new());

        Ok(empty_cached_state)
    }
}

#[cfg(any(feature = "testing", test))]
impl OsExecutionHelper<DictStateReader> {
    pub fn new_for_testing(state_reader: DictStateReader, os_block_input: OsBlockInput) -> Self {
        Self {
            cached_state: CachedState::from(state_reader),
            os_block_input,
            os_logger: OsLogger::new(true),
            tx_execution_info_iter: TxExecutionInfoIter::new(),
            call_info_index: CallInfoIndex::empty(),
        }
    }
}

struct TxExecutionInfoIter {
    next_tx_execution_infos_index: usize,
}

impl TxExecutionInfoIter {
    pub fn new() -> Self {
        Self { next_tx_execution_infos_index: 0 }
    }

    pub fn get<'a>(
        &self,
        tx_execution_infos: &'a [CentralTransactionExecutionInfo],
    ) -> Option<&'a CentralTransactionExecutionInfo> {
        tx_execution_infos.get(self.next_tx_execution_infos_index)
    }

    pub fn increment(&mut self) {
        self.next_tx_execution_infos_index += 1;
    }
}

#[derive(Clone)]
pub struct TransactionExecutionInfoForExecutionHelper {
    pub actual_fee: Fee,
    pub is_reverted: bool,
}

impl From<&CentralTransactionExecutionInfo> for TransactionExecutionInfoForExecutionHelper {
    fn from(
        tx_execution_info: &CentralTransactionExecutionInfo,
    ) -> TransactionExecutionInfoForExecutionHelper {
        TransactionExecutionInfoForExecutionHelper {
            actual_fee: tx_execution_info.actual_fee,
            is_reverted: tx_execution_info.revert_error.is_some(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionHelperError {
    #[error("Called a block execution-helper before it was initialized.")]
    NoCurrentExecutionHelper,
}
