#![warn(missing_docs)]
//! Functionality for executing Starknet transactions and contract entry points.
//!
//! In this module, we use the term "state_number" to refer to the state of the storage at the
//! execution, and "block_context_block_number" to refer to the block in which the transactions
//! should run. For example, if you want to simulate transactions at the beginning of block 10, you
//! should use state_number = 10 and block_context_block_number = 10. If you want to simulate
//! transactions at the end of block 10, you should use state_number = 11 and
//! block_context_block_number = 10.
//! See documentation of [StateNumber] for more details.
#[cfg(test)]
mod execution_test;
pub mod execution_utils;
mod state_reader;
#[cfg(test)]
mod test_utils;
#[cfg(any(feature = "testing", test))]
pub mod testing_instances;

pub mod objects;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::{StorageError, StorageReader};
use blockifier::blockifier::block::{pre_process_block, validated_gas_prices};
use blockifier::blockifier_versioned_constants::{VersionedConstants, VersionedConstantsError};
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses, TransactionContext};
use blockifier::execution::call_info::CallExecution;
use blockifier::execution::entry_point::{
    CallEntryPoint,
    CallType as BlockifierCallType,
    EntryPointExecutionContext,
    SierraGasRevertTracker,
};
use blockifier::state::cached_state::CachedState;
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::errors::TransactionExecutionError as BlockifierTransactionExecutionError;
use blockifier::transaction::objects::{
    DeprecatedTransactionInfo,
    TransactionExecutionInfo,
    TransactionInfo,
};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier::transaction::transactions::ExecutableTransaction;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::types::builtin_name::BuiltinName;
use execution_utils::{get_trace_constructor, induced_state_diff};
use objects::{PriceUnit, TransactionSimulationOutput};
use serde::{Deserialize, Serialize};
use starknet_api::block::{
    BlockHashAndNumber,
    BlockInfo,
    BlockNumber,
    NonzeroGasPrice,
    StarknetVersion,
};
use starknet_api::contract_class::{ClassInfo, EntryPointType, SierraVersion};
use starknet_api::core::{ChainId, ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::{StateNumber, ThinStateDiff};
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::{
    DeclareTransaction,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeclareTransactionV3,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
    Transaction,
    TransactionHash,
    TransactionOptions,
    TransactionVersion,
};
use starknet_api::transaction_hash::get_transaction_hash;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::Felt;
use state_reader::ExecutionStateReader;
use tokio::runtime::Handle;
use tracing::trace;

use crate::objects::{tx_execution_output_to_fee_estimation, FeeEstimation, PendingData};

/// The address of the STRK fee contract on Starknet.
const STRK_FEE_CONTRACT_ADDRESS_STR: &str =
    "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d";
/// The address of the ETH fee contract on Starknet.
const ETH_FEE_CONTRACT_ADDRESS_STR: &str =
    "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";
const DEFAULT_INITIAL_GAS_COST: u64 = 10000000000;

/// Result type for execution functions.
pub type ExecutionResult<T> = Result<T, ExecutionError>;

/// The address of the STRK fee contract on Starknet.
pub static STRK_FEE_CONTRACT_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(
        Felt::from_hex(STRK_FEE_CONTRACT_ADDRESS_STR)
            .expect("Error converting strk fee contract address from hex"),
    )
    .expect("Error converting strk fee contract address from felt")
});

/// The address of the ETH fee contract on Starknet.
pub static ETH_FEE_CONTRACT_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| {
    ContractAddress::try_from(
        Felt::from_hex(ETH_FEE_CONTRACT_ADDRESS_STR)
            .expect("Error converting eth fee contract address from hex"),
    )
    .expect("Error converting eth fee contract address from felt")
});

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq)]
/// Parameters that are needed for execution.
pub struct ExecutionConfig {
    /// The strk address to receive fees
    pub strk_fee_contract_address: ContractAddress,
    /// The eth address to receive fees
    pub eth_fee_contract_address: ContractAddress,
    /// The initial gas cost for a transaction
    pub default_initial_gas_cost: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        ExecutionConfig {
            strk_fee_contract_address: *STRK_FEE_CONTRACT_ADDRESS,
            eth_fee_contract_address: *ETH_FEE_CONTRACT_ADDRESS,
            default_initial_gas_cost: DEFAULT_INITIAL_GAS_COST,
        }
    }
}

impl SerializeConfig for ExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "strk_fee_contract_address",
                &self.strk_fee_contract_address,
                "The strk fee token address to receive fees",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "eth_fee_contract_address",
                &self.eth_fee_contract_address,
                "The eth fee token address to receive fees",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "default_initial_gas_cost",
                &self.default_initial_gas_cost,
                "The initial gas cost for a transaction",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[allow(missing_docs)]
/// The error type for the execution module.
#[derive(thiserror::Error, Debug)]
pub enum ExecutionError {
    #[error("Bad declare tx: {tx:?}. error: {err:?}")]
    BadDeclareTransaction {
        tx: Box<DeclareTransaction>,
        #[source]
        err: StarknetApiError,
    },
    #[error("Execution config file does not contain a configuration for all blocks")]
    ConfigContentError,
    #[error(transparent)]
    ConfigFileError(#[from] std::io::Error),
    #[error(transparent)]
    ConfigSerdeError(#[from] serde_json::Error),
    #[error(transparent)]
    ContractError(#[from] BlockifierError),
    #[error(
        "The contract at address {contract_address:?} is not found at state number \
         {state_number:?}."
    )]
    ContractNotFound { contract_address: ContractAddress, state_number: StateNumber },
    #[error("Gas consumed should fit into u64")]
    GasConsumedOutOfRange,
    #[error("Missing class hash in call info")]
    MissingClassHash,
    #[error("Missing compiled class with hash {class_hash} (The CASM table isn't synced)")]
    MissingCompiledClass { class_hash: ClassHash },
    #[error(transparent)]
    StateError(#[from] blockifier::state::errors::StateError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    TransactionFeeError(#[from] Box<blockifier::transaction::errors::TransactionFeeError>),
    #[error(
        "Execution failed at transaction {transaction_index:?} with error: {execution_error:?}"
    )]
    TransactionExecutionError { transaction_index: usize, execution_error: String },
    #[error("Failed to calculate transaction hash.")]
    TransactionHashCalculationFailed(StarknetApiError),
    #[error("Unknown builtin name: {builtin_name}")]
    UnknownBuiltin { builtin_name: BuiltinName },
    #[error(transparent)]
    VersionedConstants(#[from] VersionedConstantsError),
}

/// Whether the only-query bit of the transaction version is on.
pub type OnlyQuery = bool;

/// Gathers all the possible errors that can be returned from the blockifier.
type BlockifierError = anyhow::Error;

/// Executes a StarkNet call and returns the execution result.
#[allow(clippy::too_many_arguments)]
// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
pub fn execute_call(
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    chain_id: &ChainId,
    state_number: StateNumber,
    block_context_number: BlockNumber,
    contract_address: &ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
    execution_config: &ExecutionConfig,
    override_kzg_da_to_false: bool,
    class_manager_handle: Option<(SharedClassManagerClient, Handle)>,
) -> ExecutionResult<CallExecution> {
    verify_contract_exists(
        *contract_address,
        &storage_reader,
        state_number,
        maybe_pending_data.as_ref(),
    )?;

    // TODO(yair): check if this is the correct value.
    let mut remaining_gas = execution_config.default_initial_gas_cost;
    let call_entry_point = CallEntryPoint {
        class_hash: None,
        code_address: Some(*contract_address),
        entry_point_type: EntryPointType::External,
        entry_point_selector,
        calldata,
        storage_address: *contract_address,
        caller_address: ContractAddress::default(),
        call_type: BlockifierCallType::Call,
        initial_gas: remaining_gas,
    };

    let mut cached_state = CachedState::new(ExecutionStateReader {
        storage_reader: storage_reader.clone(),
        state_number,
        maybe_pending_data: maybe_pending_data.clone(),
        missing_compiled_class: Cell::new(None),
        class_manager_handle,
    });

    let block_context = create_block_context(
        &mut cached_state,
        block_context_number,
        chain_id.clone(),
        &storage_reader,
        maybe_pending_data.as_ref(),
        execution_config,
        override_kzg_da_to_false,
    )?;
    // TODO(yair): fix when supporting v3 transactions
    let tx_info = TransactionInfo::Deprecated(DeprecatedTransactionInfo::default());
    let limit_steps_by_resources = false; // Default resource bounds.

    let mut context = EntryPointExecutionContext::new_invoke(
        Arc::new(TransactionContext { block_context: Arc::new(block_context), tx_info }),
        limit_steps_by_resources,
        SierraGasRevertTracker::new(GasAmount(remaining_gas)),
    );

    let res = call_entry_point
        .execute(&mut cached_state, &mut context, &mut remaining_gas)
        .map_err(|error| {
            if let Some(class_hash) = cached_state.state.missing_compiled_class.get() {
                ExecutionError::MissingCompiledClass { class_hash }
            } else {
                ExecutionError::ContractError(error.into())
            }
        })?;

    Ok(res.execution)
}

// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
fn verify_contract_exists(
    contract_address: ContractAddress,
    storage_reader: &StorageReader,
    state_number: StateNumber,
    maybe_pending_data: Option<&PendingData>,
) -> ExecutionResult<()> {
    execution_utils::get_class_hash_at(
        &storage_reader.begin_ro_txn()?,
        state_number,
        maybe_pending_data.map(|pending_state_diff| {
            (&pending_state_diff.deployed_contracts, &pending_state_diff.replaced_classes)
        }),
        contract_address,
    )?
    .ok_or(ExecutionError::ContractNotFound { contract_address, state_number })?;
    Ok(())
}

// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
fn create_block_context(
    cached_state: &mut CachedState<ExecutionStateReader>,
    block_context_number: BlockNumber,
    chain_id: ChainId,
    storage_reader: &StorageReader,
    maybe_pending_data: Option<&PendingData>,
    execution_config: &ExecutionConfig,
    // TODO(shahak): Remove this once we stop supporting rpc v0.6.
    override_kzg_da_to_false: bool,
) -> ExecutionResult<BlockContext> {
    let (
        block_number,
        block_timestamp,
        l1_gas_price,
        l1_data_gas_price,
        l2_gas_price,
        sequencer_address,
        l1_da_mode,
    ) = match maybe_pending_data {
        Some(pending_data) => (
            block_context_number.unchecked_next(),
            pending_data.timestamp,
            pending_data.l1_gas_price,
            pending_data.l1_data_gas_price,
            pending_data.l2_gas_price,
            pending_data.sequencer,
            pending_data.l1_da_mode,
        ),
        None => {
            let header = storage_reader
                .begin_ro_txn()?
                .get_block_header(block_context_number)?
                .expect("Should have block header.")
                .block_header_without_hash;
            (
                header.block_number,
                header.timestamp,
                header.l1_gas_price,
                header.l1_data_gas_price,
                header.l2_gas_price,
                header.sequencer,
                header.l1_da_mode,
            )
        }
    };
    let ten_blocks_ago = get_10_blocks_ago(&block_context_number, cached_state)?;

    let use_kzg_da = if override_kzg_da_to_false {
        false
    } else {
        match l1_da_mode {
            L1DataAvailabilityMode::Calldata => false,
            L1DataAvailabilityMode::Blob => true,
        }
    };

    let block_info = BlockInfo {
        block_timestamp,
        sequencer_address: sequencer_address.0,
        use_kzg_da,
        block_number,
        // TODO(yair): What to do about blocks pre 0.13.1 where the data gas price were 0?
        gas_prices: validated_gas_prices(
            NonzeroGasPrice::new(l1_gas_price.price_in_wei).unwrap_or(NonzeroGasPrice::MIN),
            NonzeroGasPrice::new(l1_gas_price.price_in_fri).unwrap_or(NonzeroGasPrice::MIN),
            NonzeroGasPrice::new(l1_data_gas_price.price_in_wei).unwrap_or(NonzeroGasPrice::MIN),
            NonzeroGasPrice::new(l1_data_gas_price.price_in_fri).unwrap_or(NonzeroGasPrice::MIN),
            NonzeroGasPrice::new(l2_gas_price.price_in_wei).unwrap_or(NonzeroGasPrice::MIN),
            NonzeroGasPrice::new(l2_gas_price.price_in_fri).unwrap_or(NonzeroGasPrice::MIN),
        ),
    };
    let chain_info = ChainInfo {
        chain_id,
        fee_token_addresses: FeeTokenAddresses {
            strk_fee_token_address: execution_config.strk_fee_contract_address,
            eth_fee_token_address: execution_config.eth_fee_contract_address,
        },
        is_layer_3: false,
    };
    let starknet_version = storage_reader
        .begin_ro_txn()?
        .get_starknet_version(block_number)?
        .unwrap_or(StarknetVersion::LATEST);
    let versioned_constants = VersionedConstants::get(&starknet_version)?;

    let block_context = BlockContext::new(
        block_info,
        chain_info,
        versioned_constants.clone(),
        BouncerConfig::max(),
    );
    let next_block_number = block_context.block_info().block_number;

    pre_process_block(
        cached_state,
        ten_blocks_ago,
        next_block_number,
        &versioned_constants.os_constants,
    )?;
    Ok(block_context)
}

/// The size of the json string representing the abi of a class or deprecated class.
pub type AbiSize = usize;

/// The size of the sierra program.
pub type SierraSize = usize;

/// The size of the sierra program for deprecated contracts.
pub const DEPRECATED_CONTRACT_SIERRA_SIZE: SierraSize = 0;

/// The transaction input to be executed.
// TODO(yair): This should use broadcasted transactions instead of regular transactions, but the
// blockifier expects regular transactions. Consider changing the blockifier to use broadcasted txs.
#[allow(missing_docs)]
#[derive(Clone, Debug)]
pub enum ExecutableTransactionInput {
    Invoke(InvokeTransaction, OnlyQuery),
    // todo(yair): Do we need to support V0?
    DeclareV0(DeclareTransactionV0V1, DeprecatedContractClass, AbiSize, OnlyQuery),
    DeclareV1(DeclareTransactionV0V1, DeprecatedContractClass, AbiSize, OnlyQuery),
    DeclareV2(
        DeclareTransactionV2,
        CasmContractClass,
        SierraSize,
        AbiSize,
        OnlyQuery,
        SierraVersion,
    ),
    DeclareV3(
        DeclareTransactionV3,
        CasmContractClass,
        SierraSize,
        AbiSize,
        OnlyQuery,
        SierraVersion,
    ),
    DeployAccount(DeployAccountTransaction, OnlyQuery),
    L1Handler(L1HandlerTransaction, Fee, OnlyQuery),
}

impl ExecutableTransactionInput {
    // TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
    // instead.
    fn calc_tx_hash(self, chain_id: &ChainId) -> ExecutionResult<(Self, TransactionHash)> {
        match self.apply_on_transaction(|tx, only_query| {
            get_transaction_hash(tx, chain_id, &TransactionOptions { only_query })
        }) {
            (original_tx, Ok(tx_hash)) => Ok((original_tx, tx_hash)),
            (_, Err(err)) => Err(ExecutionError::TransactionHashCalculationFailed(err)),
        }
    }

    /// Applies a non consuming function on the transaction as if it was of type [Transaction] of
    /// StarknetAPI and returns the result without cloning the original transaction.
    // TODO(yair): Refactor this.
    fn apply_on_transaction<F, T>(self, func: F) -> (Self, T)
    where
        F: Fn(&Transaction, OnlyQuery) -> T,
    {
        match self {
            ExecutableTransactionInput::Invoke(tx, only_query) => {
                let as_transaction = Transaction::Invoke(tx);
                let res = func(&as_transaction, only_query);
                let Transaction::Invoke(tx) = as_transaction else {
                    unreachable!("Should be invoke transaction.")
                };
                (Self::Invoke(tx, only_query), res)
            }
            ExecutableTransactionInput::DeclareV0(tx, class, abi_length, only_query) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V0(tx));
                let res = func(&as_transaction, only_query);
                let Transaction::Declare(DeclareTransaction::V0(tx)) = as_transaction else {
                    unreachable!("Should be declare v0 transaction.")
                };
                (Self::DeclareV0(tx, class, abi_length, only_query), res)
            }
            ExecutableTransactionInput::DeclareV1(tx, class, abi_length, only_query) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V1(tx));
                let res = func(&as_transaction, only_query);
                let Transaction::Declare(DeclareTransaction::V1(tx)) = as_transaction else {
                    unreachable!("Should be declare v1 transaction.")
                };
                (Self::DeclareV1(tx, class, abi_length, only_query), res)
            }
            ExecutableTransactionInput::DeclareV2(
                tx,
                class,
                sierra_program_length,
                abi_length,
                only_query,
                sierra_version,
            ) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V2(tx));
                let res = func(&as_transaction, only_query);
                let Transaction::Declare(DeclareTransaction::V2(tx)) = as_transaction else {
                    unreachable!("Should be declare v2 transaction.")
                };
                (
                    Self::DeclareV2(
                        tx,
                        class,
                        sierra_program_length,
                        abi_length,
                        only_query,
                        sierra_version,
                    ),
                    res,
                )
            }
            ExecutableTransactionInput::DeclareV3(
                tx,
                class,
                sierra_program_length,
                abi_length,
                only_query,
                sierra_version,
            ) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V3(tx));
                let res = func(&as_transaction, only_query);
                let Transaction::Declare(DeclareTransaction::V3(tx)) = as_transaction else {
                    unreachable!("Should be declare v3 transaction.")
                };
                (
                    Self::DeclareV3(
                        tx,
                        class,
                        sierra_program_length,
                        abi_length,
                        only_query,
                        sierra_version,
                    ),
                    res,
                )
            }
            ExecutableTransactionInput::DeployAccount(tx, only_query) => {
                let as_transaction = Transaction::DeployAccount(tx);
                let res = func(&as_transaction, only_query);
                let Transaction::DeployAccount(tx) = as_transaction else {
                    unreachable!("Should be deploy account transaction.")
                };
                (Self::DeployAccount(tx, only_query), res)
            }
            ExecutableTransactionInput::L1Handler(tx, fee, only_query) => {
                let as_transaction = Transaction::L1Handler(tx);
                let res = func(&as_transaction, only_query);
                let Transaction::L1Handler(tx) = as_transaction else {
                    unreachable!("Should be L1 handler transaction.")
                };
                (Self::L1Handler(tx, fee, only_query), res)
            }
        }
    }

    /// Returns the transaction version.
    pub fn transaction_version(&self) -> TransactionVersion {
        match self {
            ExecutableTransactionInput::Invoke(tx, ..) => tx.version(),
            ExecutableTransactionInput::DeclareV0(..) => TransactionVersion::ZERO,
            ExecutableTransactionInput::DeclareV1(..) => TransactionVersion::ONE,
            ExecutableTransactionInput::DeclareV2(..) => TransactionVersion::TWO,
            ExecutableTransactionInput::DeclareV3(..) => TransactionVersion::THREE,
            ExecutableTransactionInput::DeployAccount(tx, ..) => tx.version(),
            ExecutableTransactionInput::L1Handler(tx, ..) => tx.version,
        }
    }
}

/// Calculates the transaction hashes for a series of transactions without cloning the transactions.
// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
fn calc_tx_hashes(
    txs: Vec<ExecutableTransactionInput>,
    chain_id: &ChainId,
) -> ExecutionResult<(Vec<ExecutableTransactionInput>, Vec<TransactionHash>)> {
    Ok(txs
        .into_iter()
        .map(|tx| tx.calc_tx_hash(chain_id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .unzip())
}

/// Output for fee estimation when a transaction reverted.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RevertedTransaction {
    /// The index of the reverted transaction.
    pub index: usize,
    /// The revert reason.
    pub revert_reason: String,
}

/// Valid output for fee estimation for a series of transactions can be either a list of fees or the
/// index and revert reason of the first reverted transaction.
pub type FeeEstimationResult = Result<Vec<FeeEstimation>, RevertedTransaction>;

/// Returns the fee estimation for a series of transactions.
#[allow(clippy::too_many_arguments)]
// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
pub fn estimate_fee(
    txs: Vec<ExecutableTransactionInput>,
    chain_id: &ChainId,
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    state_number: StateNumber,
    block_context_block_number: BlockNumber,
    execution_config: &ExecutionConfig,
    validate: bool,
    override_kzg_da_to_false: bool,
    class_manager_handle: Option<(SharedClassManagerClient, Handle)>,
) -> ExecutionResult<FeeEstimationResult> {
    let (txs_execution_info, block_context) = execute_transactions(
        txs,
        None,
        chain_id,
        storage_reader,
        maybe_pending_data,
        state_number,
        block_context_block_number,
        execution_config,
        false,
        validate,
        override_kzg_da_to_false,
        class_manager_handle,
    )?;
    let mut result = Vec::new();
    for (index, tx_execution_output) in txs_execution_info.into_iter().enumerate() {
        // If the transaction reverted, fail the entire estimation.
        if let Some(revert_reason) = tx_execution_output.execution_info.revert_error {
            return Ok(Err(RevertedTransaction {
                index,
                revert_reason: revert_reason.to_string(),
            }));
        } else {
            result
                .push(tx_execution_output_to_fee_estimation(&tx_execution_output, &block_context)?);
        }
    }
    Ok(Ok(result))
}

struct TransactionExecutionOutput {
    execution_info: TransactionExecutionInfo,
    induced_state_diff: ThinStateDiff,
    price_unit: PriceUnit,
}

// Executes a series of transactions and returns the execution results.
// TODO(yair): Return structs instead of tuples.
#[allow(clippy::too_many_arguments)]
// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
fn execute_transactions(
    txs: Vec<ExecutableTransactionInput>,
    tx_hashes: Option<Vec<TransactionHash>>,
    chain_id: &ChainId,
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    state_number: StateNumber,
    block_context_block_number: BlockNumber,
    execution_config: &ExecutionConfig,
    charge_fee: bool,
    validate: bool,
    override_kzg_da_to_false: bool,
    class_manager_handle: Option<(SharedClassManagerClient, Handle)>,
) -> ExecutionResult<(Vec<TransactionExecutionOutput>, BlockContext)> {
    // The starknet state will be from right before the block in which the transactions should run.
    let mut cached_state = CachedState::new(ExecutionStateReader {
        storage_reader: storage_reader.clone(),
        state_number,
        maybe_pending_data: maybe_pending_data.clone(),
        missing_compiled_class: Cell::new(None),
        class_manager_handle,
    });

    let block_context = create_block_context(
        &mut cached_state,
        block_context_block_number,
        chain_id.clone(),
        &storage_reader,
        maybe_pending_data.as_ref(),
        execution_config,
        override_kzg_da_to_false,
    )?;

    let (txs, tx_hashes) = match tx_hashes {
        Some(tx_hashes) => (txs, tx_hashes),
        None => {
            let tx_hashes = calc_tx_hashes(txs, chain_id)?;
            trace!("Calculated tx hashes: {:?}", tx_hashes);
            tx_hashes
        }
    };

    let mut res = vec![];
    for (transaction_index, (tx, tx_hash)) in txs.into_iter().zip(tx_hashes.into_iter()).enumerate()
    {
        let transaction_version = tx.transaction_version();
        // TODO(DanB): consider supporting match instead.
        let price_unit = if transaction_version == TransactionVersion::ZERO
            || transaction_version == TransactionVersion::ONE
            || transaction_version == TransactionVersion::TWO
        {
            PriceUnit::Wei
        } else {
            PriceUnit::Fri
        };
        let mut transactional_state = CachedState::create_transactional(&mut cached_state);
        let deprecated_declared_class_hash = match &tx {
            ExecutableTransactionInput::DeclareV0(
                DeclareTransactionV0V1 { class_hash, .. },
                _,
                _,
                _,
            ) => Some(*class_hash),
            ExecutableTransactionInput::DeclareV1(
                DeclareTransactionV0V1 { class_hash, .. },
                _,
                _,
                _,
            ) => Some(*class_hash),
            _ => None,
        };
        let blockifier_tx = to_blockifier_tx(tx, tx_hash, transaction_index, charge_fee, validate)?;
        // TODO(Yoni): use the TransactionExecutor instead.
        let tx_execution_info_result =
            blockifier_tx.execute(&mut transactional_state, &block_context);
        let state_diff =
            induced_state_diff(&mut transactional_state, deprecated_declared_class_hash)?;
        transactional_state.commit();
        let execution_info = tx_execution_info_result.map_err(|error| {
            if let Some(class_hash) = cached_state.state.missing_compiled_class.get() {
                ExecutionError::MissingCompiledClass { class_hash }
            } else {
                ExecutionError::from((transaction_index, error))
            }
        })?;
        res.push(TransactionExecutionOutput {
            execution_info,
            induced_state_diff: state_diff,
            price_unit,
        });
    }

    Ok((res, block_context))
}

/// Converts a transaction index and [BlockifierTransactionExecutionError] to an [ExecutionError].
// TODO(yair): Remove once blockifier arranges the errors hierarchy.
impl From<(usize, BlockifierTransactionExecutionError)> for ExecutionError {
    fn from(transaction_index_and_error: (usize, BlockifierTransactionExecutionError)) -> Self {
        let (transaction_index, error) = transaction_index_and_error;
        Self::TransactionExecutionError { transaction_index, execution_error: error.to_string() }
    }
}

// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
fn get_10_blocks_ago(
    block_number: &BlockNumber,
    cached_state: &CachedState<ExecutionStateReader>,
) -> ExecutionResult<Option<BlockHashAndNumber>> {
    if block_number.0 < 10 {
        return Ok(None);
    }
    let block_min_10 = BlockNumber(block_number.0 - 10);
    let Some(header_10_blocks_ago) =
        cached_state.state.storage_reader.begin_ro_txn()?.get_block_header(block_min_10)?
    else {
        return Ok(None);
    };
    Ok(Some(BlockHashAndNumber {
        number: header_10_blocks_ago.block_header_without_hash.block_number,
        hash: header_10_blocks_ago.block_hash,
    }))
}

// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
fn to_blockifier_tx(
    tx: ExecutableTransactionInput,
    tx_hash: TransactionHash,
    transaction_index: usize,
    charge_fee: bool,
    validate: bool,
) -> ExecutionResult<BlockifierTransaction> {
    // TODO(yair): support only_query version bit (enable in the RPC v0.6 and use the correct
    // value).
    let strict_nonce_check = true;
    match tx {
        ExecutableTransactionInput::Invoke(invoke_tx, only_query) => {
            let execution_flags =
                ExecutionFlags { only_query, charge_fee, validate, strict_nonce_check };
            BlockifierTransaction::from_api(
                Transaction::Invoke(invoke_tx),
                tx_hash,
                None,
                None,
                None,
                execution_flags,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }

        ExecutableTransactionInput::DeployAccount(deploy_acc_tx, only_query) => {
            let execution_flags =
                ExecutionFlags { only_query, charge_fee, validate, strict_nonce_check };
            BlockifierTransaction::from_api(
                Transaction::DeployAccount(deploy_acc_tx),
                tx_hash,
                None,
                None,
                None,
                execution_flags,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }

        ExecutableTransactionInput::DeclareV0(
            declare_tx,
            deprecated_class,
            abi_length,
            only_query,
        ) => {
            let class_info = ClassInfo::new(
                &deprecated_class.into(),
                DEPRECATED_CONTRACT_SIERRA_SIZE,
                abi_length,
                SierraVersion::DEPRECATED,
            )
            .map_err(|err| ExecutionError::BadDeclareTransaction {
                tx: DeclareTransaction::V0(declare_tx.clone()).into(),
                err,
            })?;

            let execution_flags =
                ExecutionFlags { only_query, charge_fee, validate, strict_nonce_check };
            BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V0(declare_tx)),
                tx_hash,
                Some(class_info),
                None,
                None,
                execution_flags,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
        ExecutableTransactionInput::DeclareV1(
            declare_tx,
            deprecated_class,
            abi_length,
            only_query,
        ) => {
            let class_info = ClassInfo::new(
                &deprecated_class.into(),
                DEPRECATED_CONTRACT_SIERRA_SIZE,
                abi_length,
                SierraVersion::DEPRECATED,
            )
            .map_err(|err| ExecutionError::BadDeclareTransaction {
                tx: DeclareTransaction::V1(declare_tx.clone()).into(),
                err,
            })?;
            let execution_flags =
                ExecutionFlags { only_query, charge_fee, validate, strict_nonce_check };
            BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V1(declare_tx)),
                tx_hash,
                Some(class_info),
                None,
                None,
                execution_flags,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
        ExecutableTransactionInput::DeclareV2(
            declare_tx,
            compiled_class,
            sierra_program_length,
            abi_length,
            only_query,
            sierra_version,
        ) => {
            let class_info = ClassInfo::new(
                &(compiled_class, sierra_version.clone()).into(),
                sierra_program_length,
                abi_length,
                sierra_version,
            )
            .map_err(|err| ExecutionError::BadDeclareTransaction {
                tx: DeclareTransaction::V2(declare_tx.clone()).into(),
                err,
            })?;
            let execution_flags =
                ExecutionFlags { only_query, charge_fee, validate, strict_nonce_check };
            BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V2(declare_tx)),
                tx_hash,
                Some(class_info),
                None,
                None,
                execution_flags,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
        ExecutableTransactionInput::DeclareV3(
            declare_tx,
            compiled_class,
            sierra_program_length,
            abi_length,
            only_query,
            sierra_version,
        ) => {
            let class_info = ClassInfo::new(
                &(compiled_class, sierra_version.clone()).into(),
                sierra_program_length,
                abi_length,
                sierra_version,
            )
            .map_err(|err| ExecutionError::BadDeclareTransaction {
                tx: DeclareTransaction::V3(declare_tx.clone()).into(),
                err,
            })?;
            let execution_flags =
                ExecutionFlags { only_query, charge_fee, validate, strict_nonce_check };
            BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V3(declare_tx)),
                tx_hash,
                Some(class_info),
                None,
                None,
                execution_flags,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
        ExecutableTransactionInput::L1Handler(l1_handler_tx, paid_fee, only_query) => {
            let execution_flags =
                ExecutionFlags { only_query, charge_fee, validate, strict_nonce_check };
            BlockifierTransaction::from_api(
                Transaction::L1Handler(l1_handler_tx),
                tx_hash,
                None,
                Some(paid_fee),
                None,
                execution_flags,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
    }
}

/// Simulates a series of transactions and returns the transaction traces and the fee estimations.
// TODO(yair): Return structs instead of tuples.
// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
#[allow(clippy::too_many_arguments)]
pub fn simulate_transactions(
    txs: Vec<ExecutableTransactionInput>,
    tx_hashes: Option<Vec<TransactionHash>>,
    chain_id: &ChainId,
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    state_number: StateNumber,
    block_context_block_number: BlockNumber,
    execution_config: &ExecutionConfig,
    charge_fee: bool,
    validate: bool,
    override_kzg_da_to_false: bool,
    class_manager_handle: Option<(SharedClassManagerClient, Handle)>,
) -> ExecutionResult<Vec<TransactionSimulationOutput>> {
    let trace_constructors = txs.iter().map(get_trace_constructor).collect::<Vec<_>>();
    let (execution_results, block_context) = execute_transactions(
        txs,
        tx_hashes,
        chain_id,
        storage_reader,
        maybe_pending_data,
        state_number,
        block_context_block_number,
        execution_config,
        charge_fee,
        validate,
        override_kzg_da_to_false,
        class_manager_handle,
    )?;
    execution_results
        .into_iter()
        .zip(trace_constructors)
        .map(|(tx_execution_output, trace_constructor)| {
            let fee_estimation =
                tx_execution_output_to_fee_estimation(&tx_execution_output, &block_context)?;
            match trace_constructor(tx_execution_output.execution_info) {
                Ok(transaction_trace) => Ok(TransactionSimulationOutput {
                    transaction_trace,
                    induced_state_diff: tx_execution_output.induced_state_diff,
                    fee_estimation,
                }),
                Err(e) => Err(e),
            }
        })
        .collect()
}
