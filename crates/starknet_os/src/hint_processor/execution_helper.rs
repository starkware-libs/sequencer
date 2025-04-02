use std::collections::HashMap;
use std::vec::IntoIter;

use blockifier::execution::call_info::CallInfo;
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::executable_transaction::TransactionType;
use starknet_api::transaction::fields::Fee;

use crate::errors::StarknetOsError;
use crate::hint_processor::os_logger::OsLogger;
use crate::io::os_input::{CachedStateInput, StarknetOsInput};

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<S: StateReader> {
    pub(crate) cached_state: CachedState<S>,
    pub(crate) os_input: StarknetOsInput,
    pub(crate) os_logger: OsLogger,
    tx_execution_info_iter: TxExecutionInfoIter,
}

impl<S: StateReader> OsExecutionHelper<S> {
    pub fn new(
        os_input: StarknetOsInput,
        state_reader: S,
        state_input: CachedStateInput,
        debug_mode: bool,
    ) -> Result<Self, StarknetOsError> {
        Ok(Self {
            cached_state: Self::initialize_cached_state(state_reader, state_input)?,
            os_input,
            os_logger: OsLogger::new(debug_mode),
            tx_execution_info_iter: TxExecutionInfoIter::new(),
        })
    }

    pub(crate) fn next_tx_execution_infos(
        &mut self,
        tx_type: TransactionType,
    ) -> Option<(TransactionExecutionInfoForExecutionHelper, IntoIter<CallInfo>)> {
        self.tx_execution_info_iter.next(&self.os_input.tx_execution_infos, tx_type)
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
    pub fn new_for_testing(state_reader: DictStateReader, os_input: StarknetOsInput) -> Self {
        Self {
            cached_state: CachedState::from(state_reader),
            os_input,
            os_logger: OsLogger::new(true),
            tx_execution_info_iter: TxExecutionInfoIter::new(),
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

    pub fn next(
        &mut self,
        tx_execution_infos: &[CentralTransactionExecutionInfo],
        tx_type: TransactionType,
    ) -> Option<(TransactionExecutionInfoForExecutionHelper, IntoIter<CallInfo>)> {
        let tx_execution_info = tx_execution_infos.get(self.next_tx_execution_infos_index)?;
        self.next_tx_execution_infos_index += 1;
        // TODO(Yoav): See if we can avoid cloning here.
        Some((
            tx_execution_info.into(),
            tx_execution_info.call_info_iter(tx_type).cloned().collect::<Vec<_>>().into_iter(),
        ))
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
