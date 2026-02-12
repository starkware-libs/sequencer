use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use blockifier::state::cached_state::StateMaps;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use serde::{Deserialize, Serialize};
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockHash, BlockInfo};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::core::{ChainId, CompiledClassHash, ContractAddress, OsChainInfo};
use starknet_api::transaction::{
    InvokeTransaction,
    MessageToL1,
    TransactionHash,
    TransactionHasher,
};
use starknet_os::commitment_infos::CommitmentInfo;
use starknet_os::io::os_input::{OsBlockInput, OsHints, OsHintsConfig, StarknetOsInput};
use starknet_os::runner::run_virtual_os;
use tracing::field::display;
use tracing::{info, Span};
use url::Url;

use crate::classes_provider::ClassesProvider;
use crate::errors::RunnerError;
use crate::storage_proofs::{RpcStorageProofsProvider, StorageProofConfig, StorageProofProvider};
use crate::virtual_block_executor::{
    RpcVirtualBlockExecutor,
    VirtualBlockExecutionData,
    VirtualBlockExecutor,
};

// ================================================================================================
// Virtual Os Types
// ================================================================================================

/// Virtual block input containing all non-trivial fields for OS block input construction.
pub(crate) struct VirtualOsBlockInput {
    contract_state_commitment_info: CommitmentInfo,
    address_to_storage_commitment_info: HashMap<ContractAddress, CommitmentInfo>,
    contract_class_commitment_info: CommitmentInfo,
    chain_info: OsChainInfo,
    transactions: Vec<(InvokeTransaction, TransactionHash)>,
    tx_execution_infos: Vec<CentralTransactionExecutionInfo>,
    block_info: BlockInfo,
    initial_reads: StateMaps,
    base_block_hash: BlockHash,
    base_block_header_commitments: BlockHeaderCommitments,
    prev_base_block_hash: BlockHash,
    compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
}

impl From<VirtualOsBlockInput> for OsHints {
    fn from(virtual_os_block_input: VirtualOsBlockInput) -> Self {
        let os_block_input = OsBlockInput {
            block_hash_commitments: virtual_os_block_input.base_block_header_commitments,
            contract_state_commitment_info: virtual_os_block_input.contract_state_commitment_info,
            address_to_storage_commitment_info: virtual_os_block_input
                .address_to_storage_commitment_info,
            contract_class_commitment_info: virtual_os_block_input.contract_class_commitment_info,
            transactions: virtual_os_block_input
                .transactions
                .into_iter()
                .map(|(invoke_tx, tx_hash)| {
                    starknet_api::executable_transaction::Transaction::Account(
                        starknet_api::executable_transaction::AccountTransaction::Invoke(
                            starknet_api::executable_transaction::InvokeTransaction {
                                tx: invoke_tx,
                                tx_hash,
                            },
                        ),
                    )
                })
                .collect(),
            tx_execution_infos: virtual_os_block_input.tx_execution_infos,
            prev_block_hash: virtual_os_block_input.prev_base_block_hash,
            block_info: virtual_os_block_input.block_info,
            initial_reads: virtual_os_block_input.initial_reads,
            declared_class_hash_to_component_hashes: HashMap::new(),
            new_block_hash: virtual_os_block_input.base_block_hash,
            old_block_number_and_hash: None,
            class_hashes_to_migrate: Vec::new(),
        };

        let os_input = StarknetOsInput {
            os_block_inputs: vec![os_block_input],
            deprecated_compiled_classes: BTreeMap::new(),
            compiled_classes: virtual_os_block_input.compiled_classes,
        };

        OsHints {
            os_input,
            os_hints_config: OsHintsConfig {
                debug_mode: false,
                full_output: false,
                use_kzg_da: false,
                chain_info: virtual_os_block_input.chain_info,
                public_keys: None,
                rng_seed_salt: None,
            },
        }
    }
}

// ================================================================================================
// Runner
// ================================================================================================

#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct RunnerConfig {
    /// Configuration for storage proof provider.
    pub(crate) storage_proof_config: StorageProofConfig,
}

pub(crate) struct RunnerOutput {
    pub cairo_pie: CairoPie,
    pub l2_to_l1_messages: Vec<MessageToL1>,
}

/// Generic runner for executing transactions and generating OS input.
///
/// The runner is parameterized by its providers:
/// - `C`: Classes provider for fetching compiled classes.
/// - `S`: Storage proof provider for fetching Patricia proofs.
/// - `V`: Virtual block executor for transaction execution.
#[allow(dead_code)]
pub(crate) struct Runner<C, S, V>
where
    C: ClassesProvider + Sync,
    S: StorageProofProvider + Sync,
    V: VirtualBlockExecutor,
{
    pub(crate) classes_provider: C,
    pub(crate) storage_proofs_provider: S,
    pub(crate) virtual_block_executor: V,
    pub(crate) config: RunnerConfig,
    pub(crate) contract_class_manager: ContractClassManager,
    pub(crate) block_id: BlockId,
    pub(crate) chain_id: ChainId,
}

#[allow(dead_code)]
impl<C, S, V> Runner<C, S, V>
where
    C: ClassesProvider + Sync,
    S: StorageProofProvider + Sync,
    V: VirtualBlockExecutor,
{
    pub(crate) fn new(
        classes_provider: C,
        storage_proofs_provider: S,
        virtual_block_executor: V,
        config: RunnerConfig,
        contract_class_manager: ContractClassManager,
        block_id: BlockId,
        chain_id: ChainId,
    ) -> Self {
        Self {
            classes_provider,
            storage_proofs_provider,
            virtual_block_executor,
            config,
            contract_class_manager,
            block_id,
            chain_id,
        }
    }

    /// Creates the OS hints required to run the given transactions virtually
    /// on top of the block ID specified in the runner.
    ///
    /// Takes execution data from a previous virtual block execution.
    pub(crate) async fn create_virtual_os_hints(
        execution_data: VirtualBlockExecutionData,
        classes_provider: &C,
        storage_proofs_provider: &S,
        storage_proof_config: &StorageProofConfig,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<OsHints, RunnerError> {
        // Extract chain info from block context.
        let chain_info = execution_data.base_block_info.block_context.chain_info();
        let os_chain_info = OsChainInfo {
            chain_id: chain_info.chain_id.clone(),
            strk_fee_token_address: chain_info.fee_token_addresses.strk_fee_token_address,
        };

        // Extract block number from base block info for storage proofs.
        let block_number = execution_data.base_block_info.block_context.block_info().block_number;

        // Fetch classes and storage proofs in parallel.
        let (classes, storage_proofs) = tokio::join!(
            classes_provider.get_classes(&execution_data.executed_class_hashes),
            storage_proofs_provider.get_storage_proofs(
                block_number,
                &execution_data,
                storage_proof_config
            )
        );
        let classes = classes?;
        let storage_proofs = storage_proofs?;

        // Convert execution outputs to CentralTransactionExecutionInfo.
        let tx_execution_infos =
            execution_data.execution_outputs.into_iter().map(|output| output.0.into()).collect();

        // Add class hash to compiled class hash mappings from the classes provider.
        let mut extended_initial_reads = storage_proofs.extended_initial_reads;
        extended_initial_reads
            .compiled_class_hashes
            .extend(&classes.class_hash_to_compiled_class_hash);

        // Must clear declared_contracts: the OS calls `update_cache` with an empty class map
        // (it receives compiled classes separately in `compiled_classes`), which would fail
        // an assertion if declared_contracts is non-empty.
        extended_initial_reads.declared_contracts.clear();

        // Assemble VirtualOsBlockInput.
        let virtual_os_block_input = VirtualOsBlockInput {
            contract_state_commitment_info: storage_proofs
                .commitment_infos
                .contracts_trie_commitment_info,
            address_to_storage_commitment_info: storage_proofs
                .commitment_infos
                .storage_tries_commitment_infos,
            contract_class_commitment_info: storage_proofs
                .commitment_infos
                .classes_trie_commitment_info,
            chain_info: os_chain_info,
            transactions: txs,
            tx_execution_infos,
            block_info: execution_data.base_block_info.block_context.block_info().clone(),
            initial_reads: extended_initial_reads,
            base_block_hash: execution_data.base_block_info.base_block_hash,
            base_block_header_commitments: execution_data
                .base_block_info
                .base_block_header_commitments,
            prev_base_block_hash: execution_data.base_block_info.prev_base_block_hash,
            compiled_classes: classes.compiled_classes,
        };

        // Return OsHints.
        Ok(virtual_os_block_input.into())
    }

    /// Runs the Starknet virtual OS with the given transactions.
    ///
    /// This method:
    /// 1. Executes transactions to collect state reads.
    /// 2. Fetches storage proofs and classes.
    /// 3. Builds virtual OS hints.
    /// 4. Runs the virtual OS.
    ///
    /// Consumes the runner since the virtual block executor is single-use per block.
    pub async fn run_virtual_os(
        self,
        txs: Vec<InvokeTransaction>,
    ) -> Result<RunnerOutput, RunnerError> {
        // Extract providers and executor before self is consumed.
        let Self {
            classes_provider,
            storage_proofs_provider,
            virtual_block_executor,
            config,
            contract_class_manager,
            block_id,
            chain_id,
        } = self;

        // Compute transaction hashes.
        let txs_with_hashes: Vec<(InvokeTransaction, TransactionHash)> = txs
            .into_iter()
            .map(|tx| {
                let version = tx.version();
                let tx_hash = tx
                    .calculate_transaction_hash(&chain_id, &version)
                    .map_err(|e| RunnerError::TransactionHashError(e.to_string()))?;
                // Record tx_hash on the parent span (`prove_transaction`) so all
                // subsequent logs carry it as a prefix.
                Span::current().record("tx_hash", display(&tx_hash));
                info!(transaction = ?tx, "Starting transaction proving");
                Ok((tx, tx_hash))
            })
            .collect::<Result<Vec<_>, RunnerError>>()?;

        // Clone txs since we need them after execution for VirtualOsBlockInput.
        let txs_for_hints = txs_with_hashes.clone();

        // Execute virtual block to get execution data including L2 to L1 messages.
        // Execute in a blocking thread pool to avoid blocking the async runtime.
        // The RPC state reader uses request::blocking which would block the tokio runtime.
        let execution_data = tokio::task::spawn_blocking(move || {
            virtual_block_executor.execute(block_id, contract_class_manager, txs_with_hashes)
        })
        .await??;

        // Extract L2 to L1 messages from execution data.
        let l2_to_l1_messages = execution_data.l2_to_l1_messages.clone();

        // Create OS hints from execution data.
        let os_hints = Self::create_virtual_os_hints(
            execution_data,
            &classes_provider,
            &storage_proofs_provider,
            &config.storage_proof_config,
            txs_for_hints,
        )
        .await?;

        // Run virtual OS to get Cairo PIE.
        let output = run_virtual_os(os_hints)?;

        // Construct RunnerOutput with Cairo PIE and L2 to L1 messages.
        Ok(RunnerOutput { cairo_pie: output.cairo_pie, l2_to_l1_messages })
    }
}

// ================================================================================================
// VirtualSnosRunner Trait
// ================================================================================================

/// Trait for runners that can execute the virtual Starknet OS.
///
/// This trait abstracts the execution of transactions through the virtual OS,
/// allowing different runner implementations (RPC-based, mock, etc.) to be used
/// interchangeably.
#[async_trait]
pub(crate) trait VirtualSnosRunner: Clone + Send + Sync {
    /// Runs the Starknet virtual OS with the given transactions on top of the specified block.
    async fn run_virtual_os(
        &self,
        block_id: BlockId,
        txs: Vec<InvokeTransaction>,
    ) -> Result<RunnerOutput, RunnerError>;
}

// ================================================================================================
// RPC Runner Factory
// ================================================================================================

/// Type alias for an RPC-based runner.
///
/// This runner uses:
/// - `Arc<StateReaderAndContractManager<RpcStateReader>>` for class fetching.
/// - `RpcStorageProofsProvider` for storage proofs.
/// - `RpcVirtualBlockExecutor` for transaction execution.
pub(crate) type RpcRunner = Runner<
    Arc<StateReaderAndContractManager<RpcStateReader>>,
    RpcStorageProofsProvider,
    RpcVirtualBlockExecutor,
>;

/// Factory for creating RPC-based runners.
///
/// Holds configuration that is shared across all runners:
/// - RPC node URL.
/// - Chain ID.
/// - Contract class manager (for caching compiled classes).
///
/// # Example
///
/// ```ignore
/// let factory = RpcRunnerFactory::new(
///     Url::parse("http://localhost:9545").unwrap(),
///     ChainId::Mainnet,
///     contract_class_manager,
/// );
///
/// let runner = factory.create_runner(BlockId::Number(BlockNumber(800000)));
/// let output = runner.run_virtual_os(txs).await?;
/// ```
#[derive(Clone)]
pub(crate) struct RpcRunnerFactory {
    /// URL of the RPC node.
    node_url: Url,
    /// Chain ID for the network.
    chain_id: ChainId,
    /// Contract class manager for caching compiled classes.
    contract_class_manager: ContractClassManager,
    /// Configuration for the runner.
    runner_config: RunnerConfig,
}

impl RpcRunnerFactory {
    /// Creates a new RPC runner factory.
    pub(crate) fn new(
        node_url: Url,
        chain_id: ChainId,
        contract_class_manager: ContractClassManager,
        runner_config: RunnerConfig,
    ) -> Self {
        Self { node_url, chain_id, contract_class_manager, runner_config }
    }

    /// Creates a runner configured for the given block ID.
    fn create_runner(&self, block_id: BlockId) -> RpcRunner {
        // Create the virtual block executor for this block.
        let virtual_block_executor = RpcVirtualBlockExecutor::new(
            self.node_url.to_string(),
            self.chain_id.clone(),
            block_id,
        );

        // Create the storage proofs provider.
        let storage_proofs_provider = RpcStorageProofsProvider::new(self.node_url.clone());

        // Create the state reader for class fetching.
        let rpc_state_reader = RpcStateReader::new_with_config_from_url(
            self.node_url.to_string(),
            self.chain_id.clone(),
            block_id,
        );

        // Wrap in StateReaderAndContractManager for class resolution.
        let state_reader_and_contract_manager = Arc::new(StateReaderAndContractManager::new(
            rpc_state_reader,
            self.contract_class_manager.clone(),
            None,
        ));

        Runner::new(
            state_reader_and_contract_manager,
            storage_proofs_provider,
            virtual_block_executor,
            self.runner_config.clone(),
            self.contract_class_manager.clone(),
            block_id,
            self.chain_id.clone(),
        )
    }
}

#[async_trait]
impl VirtualSnosRunner for RpcRunnerFactory {
    async fn run_virtual_os(
        &self,
        block_id: BlockId,
        txs: Vec<InvokeTransaction>,
    ) -> Result<RunnerOutput, RunnerError> {
        let runner = self.create_runner(block_id);
        runner.run_virtual_os(txs).await
    }
}
