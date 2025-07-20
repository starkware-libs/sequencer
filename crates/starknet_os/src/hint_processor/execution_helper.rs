use std::collections::HashMap;
use std::slice::Iter;

use blockifier::execution::call_info::{CallInfo, CallInfoIter};
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::BlockHash;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::executable_transaction::{AccountTransaction, Transaction, TransactionType};
use starknet_types_core::felt::Felt;

use crate::errors::StarknetOsError;
use crate::hint_processor::os_logger::OsLogger;
use crate::hint_processor::snos_hint_processor::{
    DeprecatedSyscallHintProcessor,
    SyscallHintProcessor,
};
use crate::io::os_input::{CachedStateInput, OsBlockInput};
use crate::vm_utils::VmUtilsError;

/// A helper struct that provides access to the OS state and commitments.
pub struct OsExecutionHelper<'a, S: StateReader> {
    pub(crate) cached_state: CachedState<S>,
    pub(crate) os_block_input: &'a OsBlockInput,
    pub(crate) class_hashes_to_migrate_iterator: Box<dyn Iterator<Item = ClassHash> + 'a>,
    pub(crate) os_logger: OsLogger,
    pub(crate) tx_execution_iter: TransactionExecutionIter<'a>,
    pub(crate) tx_tracker: TransactionTracker<'a>,
    pub(crate) syscall_hint_processor: SyscallHintProcessor,
    pub(crate) deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,
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
            class_hashes_to_migrate_iterator: Box::new(
                os_block_input.class_hashes_to_migrate.keys().cloned(),
            ),
            os_logger: OsLogger::new(debug_mode),
            tx_execution_iter: TransactionExecutionIter::new(&os_block_input.tx_execution_infos),
            tx_tracker: TransactionTracker::new(&os_block_input.transactions),
            syscall_hint_processor: SyscallHintProcessor::default(),
            deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor::default(),
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
            class_hashes_to_migrate_iterator: Box::new(
                os_block_input.class_hashes_to_migrate.keys().cloned(),
            ),
            os_logger: OsLogger::new(true),
            tx_execution_iter: TransactionExecutionIter::new(&os_block_input.tx_execution_infos),
            tx_tracker: TransactionTracker::new(&os_block_input.transactions),
            syscall_hint_processor: SyscallHintProcessor::default(),
            deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor::default(),
        }
    }
}

pub struct TransactionExecutionInfoReference<'a> {
    pub tx_execution_info: &'a CentralTransactionExecutionInfo,
    call_info_iter: CallInfoIter<'a>,
    pub call_info_tracker: Option<CallInfoTracker<'a>>,
}

impl<'a> TransactionExecutionInfoReference<'a> {
    pub fn new(
        tx_execution_info: &'a CentralTransactionExecutionInfo,
        tx_type: TransactionType,
    ) -> Self {
        Self {
            tx_execution_info,
            call_info_iter: tx_execution_info.call_info_iter(tx_type),
            call_info_tracker: None,
        }
    }

    pub fn enter_call(
        &mut self,
        execution_info_ptr: Relocatable,
        deprecated_tx_info_ptr: Relocatable,
    ) -> Result<(), ExecutionHelperError> {
        if self.call_info_tracker.is_some() {
            return Err(ExecutionHelperError::ContextOverwrite {
                context: "call info".to_string(),
            });
        }
        let next_call_info = self
            .call_info_iter
            .next()
            .ok_or(ExecutionHelperError::EndOfIterator { item_type: "call_info".to_string() })?;
        self.call_info_tracker =
            Some(CallInfoTracker::new(next_call_info, execution_info_ptr, deprecated_tx_info_ptr));
        Ok(())
    }

    pub fn exit_call_info(&mut self) -> Result<(), ExecutionHelperError> {
        self.get_mut_call_info_tracker()?.verify_exhausted_iterators()?;
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

    pub fn start_tx(&mut self, tx_type: TransactionType) -> Result<(), ExecutionHelperError> {
        if self.tx_execution_info_ref.is_some() {
            return Err(ExecutionHelperError::ContextOverwrite {
                context: "transaction execution info".to_string(),
            });
        }
        let next_tx_execution_info = self
            .tx_execution_info_iter
            .next()
            .ok_or(ExecutionHelperError::MissingTxExecutionInfo)?;
        self.tx_execution_info_ref =
            Some(TransactionExecutionInfoReference::new(next_tx_execution_info, tx_type));
        Ok(())
    }

    pub fn end_tx(&mut self) -> Result<(), ExecutionHelperError> {
        if self.get_mut_tx_execution_info_ref()?.call_info_iter.next().is_some() {
            return Err(ExecutionHelperError::UnexhaustedCallInfoIterator);
        }
        self.tx_execution_info_ref = None;
        Ok(())
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

    pub fn skip_tx(&mut self) -> Result<(), ExecutionHelperError> {
        // The transaction type determines in which order the inner calls were executed.
        // Since the transaction is skipped, the transaction's type doesn't matter.
        let dummy_tx_type = TransactionType::Declare;
        self.start_tx(dummy_tx_type)?;
        self.end_tx()?;
        Ok(())
    }
}

pub struct CallInfoTracker<'a> {
    pub call_info: &'a CallInfo,
    pub deployed_contracts_iterator: Box<dyn Iterator<Item = ContractAddress> + 'a>,
    pub inner_calls_iterator: Iter<'a, CallInfo>,
    pub execute_code_read_iterator: Iter<'a, Felt>,
    pub execute_code_class_hash_read_iterator: Iter<'a, ClassHash>,
    pub execute_code_block_hash_read_iterator: Iter<'a, BlockHash>,
    pub execution_info_ptr: Relocatable,
    pub deprecated_tx_info_ptr: Relocatable,
}

macro_rules! next_iterator_method {
    ($fn_name:ident, $field:ident, $return_type:ty) => {
        pub fn $fn_name(&mut self) -> Result<$return_type, ExecutionHelperError> {
            self.$field.next().ok_or(ExecutionHelperError::EndOfIterator {
                item_type: stringify!($field).to_string(),
            })
        }
    };
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
                    .map(|inner| inner.call.storage_address),
            ),
            inner_calls_iterator: call_info.inner_calls.iter(),
            execute_code_read_iterator: call_info.storage_access_tracker.storage_read_values.iter(),
            execute_code_class_hash_read_iterator: call_info
                .storage_access_tracker
                .read_class_hash_values
                .iter(),
            execute_code_block_hash_read_iterator: call_info
                .storage_access_tracker
                .read_block_hash_values
                .iter(),
            execution_info_ptr,
            deprecated_tx_info_ptr,
        }
    }

    pub fn verify_exhausted_iterators(&mut self) -> Result<(), ExecutionHelperError> {
        let mut unexhausteds_iterators = Vec::new();

        check_exhausted(
            &mut self.deployed_contracts_iterator,
            "deployed_contracts_iterator",
            &mut unexhausteds_iterators,
        );
        check_exhausted(
            &mut self.inner_calls_iterator,
            "inner_calls_iterator",
            &mut unexhausteds_iterators,
        );
        check_exhausted(
            &mut self.execute_code_read_iterator,
            "execute_code_read_iterator",
            &mut unexhausteds_iterators,
        );
        check_exhausted(
            &mut self.execute_code_class_hash_read_iterator,
            "execute_code_class_hash_read_iterator",
            &mut unexhausteds_iterators,
        );
        check_exhausted(
            &mut self.execute_code_block_hash_read_iterator,
            "execute_code_block_hash_read_iterator",
            &mut unexhausteds_iterators,
        );

        if !unexhausteds_iterators.is_empty() {
            return Err(ExecutionHelperError::UnexhaustedCallInfoDataIterators {
                iters: unexhausteds_iterators,
            });
        }
        Ok(())
    }

    next_iterator_method!(next_execute_code_read, execute_code_read_iterator, &Felt);
    next_iterator_method!(
        next_execute_code_class_hash_read,
        execute_code_class_hash_read_iterator,
        &ClassHash
    );
    next_iterator_method!(
        next_execute_code_block_hash_read,
        execute_code_block_hash_read_iterator,
        &BlockHash
    );
    next_iterator_method!(next_inner_call, inner_calls_iterator, &CallInfo);
    next_iterator_method!(
        next_deployed_contracts_iterator,
        deployed_contracts_iterator,
        ContractAddress
    );
}

fn check_exhausted<I>(iterator: &mut I, name: &str, unexhausteds: &mut Vec<String>)
where
    I: Iterator,
{
    if iterator.next().is_some() {
        unexhausteds.push(name.to_string());
    }
}

pub struct TransactionTracker<'a> {
    txs_iter: Iter<'a, Transaction>,
    pub tx_ref: Option<&'a Transaction>,
}

impl<'a> TransactionTracker<'a> {
    pub fn new(txs: &'a [Transaction]) -> Self {
        Self { txs_iter: txs.iter(), tx_ref: None }
    }

    pub fn load_next_tx(&mut self) -> Result<&'a Transaction, ExecutionHelperError> {
        let next_tx = self
            .txs_iter
            .next()
            .ok_or(ExecutionHelperError::EndOfIterator { item_type: "transaction".to_string() })?;
        self.tx_ref = Some(next_tx);
        Ok(next_tx)
    }

    pub fn get_tx(&self) -> Result<&'a Transaction, ExecutionHelperError> {
        self.tx_ref.ok_or(ExecutionHelperError::MissingTx)
    }

    pub fn get_account_tx(&self) -> Result<&'a AccountTransaction, ExecutionHelperError> {
        let tx = self.get_tx()?;
        match tx {
            Transaction::Account(account_transaction) => Ok(account_transaction),
            Transaction::L1Handler(_) => Err(ExecutionHelperError::UnexpectedTxType(tx.tx_type())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionHelperError {
    #[error("Attempted to overwrite an active context: {context}")]
    ContextOverwrite { context: String },
    #[error("Tried to iterate past the end of {item_type}.")]
    EndOfIterator { item_type: String },
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error("No call info found.")]
    MissingCallInfo,
    #[error("No commitment info for contract address: {0:?}.")]
    MissingCommitmentInfo(ContractAddress),
    #[error("No transaction found.")]
    MissingTx,
    #[error("No transaction execution info found.")]
    MissingTxExecutionInfo,
    #[error("Called a block execution-helper before it was initialized.")]
    NoCurrentExecutionHelper,
    #[error("Exit call info before exhausting data iterators {iters:?}.")]
    UnexhaustedCallInfoDataIterators { iters: Vec<String> },
    #[error("Exit transaction before exhausting call info iterator.")]
    UnexhaustedCallInfoIterator,
    #[error("Unexpected tx type: {0:?}.")]
    UnexpectedTxType(TransactionType),
    #[error(transparent)]
    VmUtils(#[from] VmUtilsError),
}
