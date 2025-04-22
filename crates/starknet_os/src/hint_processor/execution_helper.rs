use std::collections::HashMap;
use std::slice::Iter;

use blockifier::execution::call_info::{CallInfo, CallInfoIter};
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::executable_transaction::TransactionType;

use crate::errors::StarknetOsError;
use crate::hint_processor::os_logger::OsLogger;
use crate::io::os_input::{CachedStateInput, OsBlockInput};

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<'a, S: StateReader> {
    pub(crate) cached_state: CachedState<S>,
    pub(crate) os_block_input: &'a OsBlockInput,
    pub(crate) os_logger: OsLogger,
    pub(crate) tx_execution_iter: TransactionExecutionIter<'a>,
}

impl<'a, S: StateReader> OsExecutionHelper<'a, S> {
    pub fn new(
        os_block_input: &'a OsBlockInput,
        state_reader: S,
        state_input: CachedStateInput,
        debug_mode: bool,
    ) -> Result<Self, StarknetOsError> {
        Ok(Self {
            cached_state: Self::initialize_cached_state(state_reader, state_input)?,
            os_block_input,
            os_logger: OsLogger::new(debug_mode),
            tx_execution_iter: TransactionExecutionIter::new(&os_block_input.tx_execution_infos),
        })
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
impl<'a> OsExecutionHelper<'a, DictStateReader> {
    pub fn new_for_testing(
        state_reader: DictStateReader,
        os_block_input: &'a OsBlockInput,
    ) -> Self {
        Self {
            cached_state: CachedState::from(state_reader),
            os_block_input,
            os_logger: OsLogger::new(true),
            tx_execution_iter: TransactionExecutionIter::new(&os_block_input.tx_execution_infos),
        }
    }
}

pub struct TransactionExecutionInfoReference<'a> {
    pub tx_execution_info: &'a CentralTransactionExecutionInfo,
    call_info_iter: CallInfoIter<'a>,
    pub call_info: Option<&'a CallInfo>,
}

impl TransactionExecutionInfoReference<'_> {
    pub fn next_call_info(&mut self) -> Option<()> {
        self.call_info = self.call_info_iter.next();
        self.call_info.map(|_| ())
    }
}

pub struct TransactionExecutionIter<'a> {
    tx_execution_info_iter: Iter<'a, CentralTransactionExecutionInfo>,
    pub tx_execution_info_ref: Option<TransactionExecutionInfoReference<'a>>,
}

impl<'a> TransactionExecutionIter<'a> {
    pub fn new(tx_execution_infos: &'a [CentralTransactionExecutionInfo]) -> Self {
        Self { tx_execution_info_iter: tx_execution_infos.iter(), tx_execution_info_ref: None }
    }

    pub fn next_tx(&mut self, tx_type: TransactionType) -> Option<()> {
        self.tx_execution_info_iter.next().map(|tx_execution_info| {
            self.tx_execution_info_ref = Some(TransactionExecutionInfoReference {
                tx_execution_info,
                call_info_iter: tx_execution_info.call_info_iter(tx_type),
                call_info: None,
            });
        })
    }

    pub fn next_call_info(&mut self) -> Option<()> {
        self.tx_execution_info_ref.as_mut().and_then(|v| v.next_call_info())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionHelperError {
    #[error("Called a block execution-helper before it was initialized.")]
    NoCurrentExecutionHelper,
}
