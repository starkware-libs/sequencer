use std::collections::HashMap;
use std::slice::Iter;

use blockifier::execution::call_info::{CallInfo, CallInfoIter};
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_vm::types::relocatable::Relocatable;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::executable_transaction::TransactionType;
use starknet_types_core::felt::Felt;

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
    pub call_info_tracker: Option<CallInfoTracker<'a>>,
}

impl<'a> TransactionExecutionInfoReference<'a> {
    pub fn next_call_info(
        &mut self,
        execution_info_ptr: Relocatable,
        deprecated_tx_info_ptr: Relocatable,
    ) -> Option<()> {
        self.call_info_tracker = Some(CallInfoTracker::new(
            self.call_info_iter.next()?,
            execution_info_ptr,
            deprecated_tx_info_ptr,
        ));
        Some(())
    }

    pub fn exit_call_info(&mut self) -> Result<(), ExecutionHelperError> {
        self.get_mut_call_info_tracker()?.assert_exhausted_iterators()?;
        self.call_info_tracker = None;
        Ok(())
    }

    pub fn get_call_info_tracker(&self) -> Result<&CallInfoTracker<'a>, ExecutionHelperError> {
        self.call_info_tracker.as_ref().ok_or(ExecutionHelperError::MissingCallInfo)
    }

    pub fn get_mut_call_info_tracker(
        &mut self,
    ) -> Result<&mut CallInfoTracker<'a>, ExecutionHelperError> {
        self.call_info_tracker.as_mut().ok_or(ExecutionHelperError::MissingCallInfo)
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
                call_info_tracker: None,
            });
        })
    }

    pub fn get_tx_execution_info_ref(
        &self,
    ) -> Result<&TransactionExecutionInfoReference<'a>, ExecutionHelperError> {
        self.tx_execution_info_ref.as_ref().ok_or(ExecutionHelperError::MissingTxExecutionInfo)
    }

    pub fn get_mut_tx_execution_info_ref(
        &mut self,
    ) -> Result<&mut TransactionExecutionInfoReference<'a>, ExecutionHelperError> {
        self.tx_execution_info_ref.as_mut().ok_or(ExecutionHelperError::MissingTxExecutionInfo)
    }
}

pub struct CallInfoTracker<'a> {
    pub call_info: &'a CallInfo,
    pub deployed_contracts_iterator: Box<dyn Iterator<Item = ContractAddress> + 'a>,
    pub inner_calls_iterator: Iter<'a, CallInfo>,
    pub execute_code_read_iterator: Iter<'a, Felt>,
    pub execute_code_class_hash_read_iterator: Iter<'a, ClassHash>,
    pub execution_info_ptr: Relocatable,
    pub deprecated_tx_info_ptr: Relocatable,
}

impl<'a> CallInfoTracker<'a> {
    pub fn new(
        call_info: &'a CallInfo,
        execution_info_ptr: Relocatable,
        deprecated_tx_info_ptr: Relocatable,
    ) -> Self {
        Self {
            call_info,
            deployed_contracts_iterator: Box::new(
                call_info
                    .inner_calls
                    .iter()
                    .filter(|inner| inner.call.entry_point_type == EntryPointType::Constructor)
                    .map(|inner| inner.call.caller_address),
            ),
            inner_calls_iterator: call_info.inner_calls.iter(),
            execute_code_read_iterator: call_info.storage_access_tracker.storage_read_values.iter(),
            execute_code_class_hash_read_iterator: call_info
                .storage_access_tracker
                .read_class_hash_values
                .iter(),
            execution_info_ptr,
            deprecated_tx_info_ptr,
        }
    }

    pub fn assert_exhausted_iterators(&mut self) -> Result<(), ExecutionHelperError> {
        let mut iterators = Vec::new();

        check_exhausted(
            &mut self.deployed_contracts_iterator,
            "deployed_contracts_iterator",
            &mut iterators,
        );
        check_exhausted(&mut self.inner_calls_iterator, "inner_calls_iterator", &mut iterators);
        check_exhausted(
            &mut self.execute_code_read_iterator,
            "execute_code_read_iterator",
            &mut iterators,
        );
        check_exhausted(
            &mut self.execute_code_class_hash_read_iterator,
            "execute_code_class_hash_read_iterator",
            &mut iterators,
        );

        if !iterators.is_empty() {
            return Err(ExecutionHelperError::UnexhaustedCallInfoDataIterators {
                iters: iterators,
            });
        }
        Ok(())
    }
}

fn check_exhausted<I>(iterator: &mut I, name: &str, iterators: &mut Vec<String>)
where
    I: Iterator,
{
    if iterator.next().is_some() {
        iterators.push(name.to_string());
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionHelperError {
    #[error("No call info found.")]
    MissingCallInfo,
    #[error("No transaction execution info found.")]
    MissingTxExecutionInfo,
    #[error("Called a block execution-helper before it was initialized.")]
    NoCurrentExecutionHelper,
    #[error("Exit call info before exhausting data iterators {iters:?}.")]
    UnexhaustedCallInfoDataIterators { iters: Vec<String> },
}
