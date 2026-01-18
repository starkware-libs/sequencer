use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use blockifier::state::cached_state::StateMaps;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::core::{ChainId, CompiledClassHash, ContractAddress};
use starknet_api::transaction::{InvokeTransaction, TransactionHash};
use starknet_os::io::os_input::{
    CommitmentInfo,
    OsBlockInput,
    OsChainInfo,
    OsHints,
    OsHintsConfig,
    StarknetOsInput,
};
use starknet_os::io::virtual_os_output::VirtualOsRunnerOutput;
use starknet_os::runner::run_virtual_os;
use url::Url;

use crate::classes_provider::ClassesProvider;
use crate::errors::RunnerError;
use crate::storage_proofs::{RpcStorageProofsProvider, StorageProofProvider};
use crate::virtual_block_executor::{RpcVirtualBlockExecutor, VirtualBlockExecutor};

// ================================================================================================
// Virtual Os Types
// ================================================================================================

/// Virtual block input containing all non-trivial fields for OS block input construction.
pub struct VirtualOsBlockInput {
    pub contract_state_commitment_info: CommitmentInfo,
    pub address_to_storage_commitment_info: HashMap<ContractAddress, CommitmentInfo>,
    pub contract_class_commitment_info: CommitmentInfo,
    pub chain_info: OsChainInfo,
    pub transactions: Vec<(InvokeTransaction, TransactionHash)>,
    pub tx_execution_infos: Vec<CentralTransactionExecutionInfo>,
    pub block_info: BlockInfo,
    pub initial_reads: StateMaps,
    pub base_block_hash: BlockHash,
    pub base_block_header_commitments: BlockHeaderCommitments,
    pub prev_base_block_hash: BlockHash,
    pub compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
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

/// Generic runner for executing transactions and generating OS input.
///
/// The runner is parameterized by its providers:
/// - `C`: Classes provider for fetching compiled classes
/// - `S`: Storage proof provider for fetching Patricia proofs
/// - `V`: Virtual block executor for transaction execution
pub struct Runner<C, S, V>
where
    C: ClassesProvider + Sync,
    S: StorageProofProvider + Sync,
    V: VirtualBlockExecutor,
{
    pub classes_provider: C,
    pub storage_proofs_provider: S,
    pub virtual_block_executor: V,
    pub(crate) contract_class_manager: ContractClassManager,
    pub(crate) block_number: BlockNumber,
}

impl<C, S, V> Runner<C, S, V>
where
    C: ClassesProvider + Sync,
    S: StorageProofProvider + Sync,
    V: VirtualBlockExecutor,
{
    pub fn new(
        classes_provider: C,
        storage_proofs_provider: S,
        virtual_block_executor: V,
        contract_class_manager: ContractClassManager,
        block_number: BlockNumber,
    ) -> Self {
        Self {
            classes_provider,
            storage_proofs_provider,
            virtual_block_executor,
            contract_class_manager,
            block_number,
        }
    }

    /// Creates the OS hints required to run the given transactions virtually
    /// on top of the block number specified in the runner.
    ///
    /// Consumes the runner.
    pub async fn create_os_hints(
        self,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<OsHints, RunnerError> {
        // Destructure self to move executor into spawn_blocking while keeping providers.
        let Self {
            classes_provider,
            storage_proofs_provider,
            virtual_block_executor,
            contract_class_manager,
            block_number,
        } = self;

        // Clone txs since we need them after spawn_blocking for VirtualOsBlockInput.
        let txs_for_execute = txs.clone();

        // Execute virtual block in a blocking thread pool to avoid blocking the async runtime.
        // The RPC state reader uses request::blocking which would block the tokio runtime.
        let mut execution_data = tokio::task::spawn_blocking(move || {
            virtual_block_executor.execute(block_number, contract_class_manager, txs_for_execute)
        })
        .await??;

        // Extract chain info from block context.
        let chain_info = execution_data.base_block_info.block_context.chain_info();
        let os_chain_info = OsChainInfo {
            chain_id: chain_info.chain_id.clone(),
            strk_fee_token_address: chain_info.fee_token_addresses.strk_fee_token_address,
        };

        // Fetch classes and storage proofs in parallel.
        let (classes, storage_proofs) = tokio::join!(
            classes_provider.get_classes(&execution_data.executed_class_hashes),
            storage_proofs_provider.get_storage_proofs(block_number, &execution_data)
        );
        let classes = classes?;
        let storage_proofs = storage_proofs?;

        // Convert execution outputs to CentralTransactionExecutionInfo.
        let tx_execution_infos =
            execution_data.execution_outputs.into_iter().map(|output| output.0.into()).collect();

        // Merge initial_reads with proof_state.
        execution_data.initial_reads.extend(&storage_proofs.proof_state);

        // Add class hash to compiled class hash mappings from the classes provider.
        execution_data
            .initial_reads
            .compiled_class_hashes
            .extend(&classes.class_hash_to_compiled_class_hash);

        // Clear declared_contracts - not used by the OS.
        execution_data.initial_reads.declared_contracts.clear();

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
            initial_reads: execution_data.initial_reads,
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

    /// Runs the Starknet OS with the given transactions.
    ///
    /// This method:
    /// 1. Executes transactions to collect state reads
    /// 2. Fetches storage proofs and classes
    /// 3. Builds OS hints
    /// 4. Runs the OS in stateless mode (all state pre-loaded in input)
    ///
    /// Consumes the runner since the virtual block executor is single-use per block.
    ///
    /// Returns the OS output containing the Cairo PIE and execution metrics.
    pub async fn run_os(
        self,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<VirtualOsRunnerOutput, RunnerError> {
        let os_hints = self.create_os_hints(txs).await?;
        let output = run_virtual_os(os_hints)?;
        Ok(output)
    }
}

// ================================================================================================
// RPC Runner Factory
// ================================================================================================

/// Type alias for an RPC-based runner.
///
/// This runner uses:
/// - `Arc<StateReaderAndContractManager<RpcStateReader>>` for class fetching
/// - `RpcStorageProofsProvider` for storage proofs
/// - `RpcVirtualBlockExecutor` for transaction execution
pub type RpcRunner = Runner<
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
/// let runner = factory.create_runner(BlockNumber(800000));
/// let output = runner.run_os(txs).await?;
/// ```
pub struct RpcRunnerFactory {
    /// URL of the RPC node.
    pub node_url: Url,
    /// Chain ID for the network.
    pub chain_id: ChainId,
    /// Contract class manager for caching compiled classes.
    pub contract_class_manager: ContractClassManager,
}

impl RpcRunnerFactory {
    /// Creates a new RPC runner factory.
    pub fn new(
        node_url: Url,
        chain_id: ChainId,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        Self { node_url, chain_id, contract_class_manager }
    }

    /// Creates a runner configured for the given block number.
    ///
    /// The runner is ready to execute transactions on top of the specified block.
    /// Each runner is single-use (consumed when `run_os` is called).
    pub fn create_runner(&self, block_number: BlockNumber) -> RpcRunner {
        // Create the virtual block executor for this block.
        let virtual_block_executor = RpcVirtualBlockExecutor::new(
            self.node_url.to_string(),
            self.chain_id.clone(),
            block_number,
        );

        // Create the storage proofs provider.
        let storage_proofs_provider = RpcStorageProofsProvider::new(self.node_url.clone());

        // Create the state reader for class fetching.
        let rpc_state_reader = RpcStateReader::new_with_config_from_url(
            self.node_url.to_string(),
            self.chain_id.clone(),
            block_number,
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
            self.contract_class_manager.clone(),
            block_number,
        )
    }
}
