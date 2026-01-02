use std::collections::HashMap;
use std::fs::read_to_string;
use std::sync::{Arc, Mutex};

use apollo_gateway::errors::{serde_err_to_state_err, RPCStateReaderError};
use apollo_gateway::rpc_objects::{BlockHeader, BlockId, GetBlockWithTxHashesParams};
use apollo_gateway::rpc_state_reader::RpcStateReader as GatewayRpcStateReader;
use apollo_gateway_config::config::RpcStateReaderConfig;
use assert_matches::assert_matches;
use blockifier::abi::constants;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutionOutput,
    TransactionExecutor,
};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::{CommitmentStateDiff, StateMaps};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::StateError;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use serde::Serialize;
use serde_json::{json, Value};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockInfo, BlockNumber, StarknetVersion};
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_core::types::ContractClass as StarknetContractClass;
use starknet_types_core::felt::Felt;

use crate::cli::TransactionInput;
use crate::compile::{legacy_to_contract_class_v0, sierra_to_versioned_contract_class_v1};
use crate::errors::ReexecutionResult;
use crate::retry_request;
use crate::serde_utils::{
    deserialize_transaction_json_to_starknet_api_tx,
    hashmap_from_raw,
    nested_hashmap_from_raw,
};
use crate::state_reader::offline_state_reader::{
    SerializableDataNextBlock,
    SerializableDataPrevBlock,
    SerializableOfflineReexecutionData,
};
use crate::state_reader::reexecution_state_reader::{
    ConsecutiveReexecutionStateReaders,
    ReexecutionStateReader,
    DUMMY_COMPILED_CLASS_HASH,
};
use crate::utils::{
    disjoint_hashmap_union,
    get_chain_info,
    get_rpc_state_reader_config,
    ComparableStateDiff,
};

pub const DEFAULT_RETRY_COUNT: usize = 3;
pub const DEFAULT_RETRY_WAIT_TIME: u64 = 10000;
pub const DEFAULT_EXPECTED_ERROR_STRINGS: [&str; 3] =
    ["Connection error", "RPCError", "429 Too Many Requests"];
pub const DEFAULT_RETRY_FAILURE_MESSAGE: &str = "Failed to connect to the RPC node.";

pub type StarknetContractClassMapping = HashMap<ClassHash, StarknetContractClass>;

// TODO(Aviv): Consider moving to the gateway state reader.
/// Params for the RPC request "starknet_getTransactionByHash".
#[derive(Serialize)]
pub struct GetTransactionByHashParams {
    pub transaction_hash: String,
}

#[derive(Clone)]
pub struct RetryConfig {
    pub(crate) n_retries: usize,
    pub(crate) retry_interval_milliseconds: u64,
    pub(crate) expected_error_strings: Vec<&'static str>,
    pub(crate) retry_failure_message: &'static str,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            n_retries: DEFAULT_RETRY_COUNT,
            retry_interval_milliseconds: DEFAULT_RETRY_WAIT_TIME,
            expected_error_strings: DEFAULT_EXPECTED_ERROR_STRINGS.to_vec(),
            retry_failure_message: DEFAULT_RETRY_FAILURE_MESSAGE,
        }
    }
}

#[derive(Clone)]
pub struct RpcStateReader {
    pub rpc_state_reader: GatewayRpcStateReader,
    pub(crate) retry_config: RetryConfig,
    pub chain_id: ChainId,
    #[allow(dead_code)]
    pub(crate) contract_class_mapping_dumper: Arc<Mutex<Option<StarknetContractClassMapping>>>,
}

impl Default for RpcStateReader {
    fn default() -> Self {
        Self {
            rpc_state_reader: GatewayRpcStateReader::from_latest(&get_rpc_state_reader_config()),
            retry_config: RetryConfig::default(),
            chain_id: ChainId::Mainnet,
            contract_class_mapping_dumper: Arc::new(Mutex::new(None)),
        }
    }
}

impl StateReader for RpcStateReader {
    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        retry_request!(self.retry_config, || self.rpc_state_reader.get_nonce_at(contract_address))
    }

    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        retry_request!(self.retry_config, || self
            .rpc_state_reader
            .get_storage_at(contract_address, key))
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        retry_request!(self.retry_config, || self
            .rpc_state_reader
            .get_class_hash_at(contract_address))
    }

    /// Returns the contract class of the given class hash.
    /// Compile the contract class if it is Sierra.
    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let contract_class =
            retry_request!(self.retry_config, || self.get_contract_class(&class_hash))?;

        match contract_class {
            StarknetContractClass::Sierra(sierra) => {
                let sierra_contract = SierraContractClass::from(sierra);
                let (casm, _) = sierra_to_versioned_contract_class_v1(sierra_contract).unwrap();
                Ok(RunnableCompiledClass::try_from(casm).unwrap())
            }
            StarknetContractClass::Legacy(legacy) => {
                Ok(legacy_to_contract_class_v0(legacy).unwrap().try_into().unwrap())
            }
        }
    }

    /// Returns a dummy compiled class hash for reexecution purposes.
    ///
    /// This method is required since v0.14.1 for checking if compiled class hashes
    /// need to be migrated from v1 to v2 format.
    /// In reexecution we use a dummy value for both get_compiled_class_hash and
    /// get_compiled_class_hash_v2, to avoid the migration process.
    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        Ok(DUMMY_COMPILED_CLASS_HASH)
    }

    /// returns the same value as get_compiled_class_hash, to avoid the migration process.
    fn get_compiled_class_hash_v2(
        &self,
        class_hash: ClassHash,
        _compiled_class: &RunnableCompiledClass,
    ) -> StateResult<CompiledClassHash> {
        self.get_compiled_class_hash(class_hash)
    }
}

impl FetchCompiledClasses for RpcStateReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        let contract_class =
            retry_request!(self.retry_config, || self.get_contract_class(&class_hash))?;

        self.starknet_core_contract_class_to_compiled_class(contract_class)
    }

    /// This check is no needed in the reexecution context.
    /// We assume that all the classes returned successfuly by the rpc-provider are declared.
    fn is_declared(&self, _class_hash: ClassHash) -> StateResult<bool> {
        Ok(true)
    }
}

impl RpcStateReader {
    pub fn new(
        config: &RpcStateReaderConfig,
        chain_id: ChainId,
        block_number: BlockNumber,
        dump_mode: bool,
    ) -> Self {
        let contract_class_mapping_dumper = Arc::new(Mutex::new(match dump_mode {
            true => Some(HashMap::new()),
            false => None,
        }));
        Self {
            rpc_state_reader: GatewayRpcStateReader::from_number(config, block_number),
            retry_config: RetryConfig::default(),
            chain_id,
            contract_class_mapping_dumper,
        }
    }

    /// Creates an RpcStateReader from a node URL, chain ID, and block number.
    pub fn new_with_config_from_url(
        node_url: String,
        chain_id: ChainId,
        block_number: BlockNumber,
    ) -> Self {
        let config = RpcStateReaderConfig::from_url(node_url);
        Self::new(&config, chain_id, block_number, false)
    }

    pub fn new_for_testing(block_number: BlockNumber) -> Self {
        RpcStateReader::new(&get_rpc_state_reader_config(), ChainId::Mainnet, block_number, false)
    }

    /// Get the block header of the current block.
    pub fn get_block_header(&self) -> ReexecutionResult<BlockHeader> {
        let json = retry_request!(self.retry_config, || {
            self.rpc_state_reader.send_rpc_request(
                "starknet_getBlockWithTxHashes",
                GetBlockWithTxHashesParams { block_id: self.rpc_state_reader.block_id },
            )
        })?;

        Ok(serde_json::from_value::<BlockHeader>(json)?)
    }

    /// Get the block info of the current block.
    pub fn get_block_info(&self) -> ReexecutionResult<BlockInfo> {
        Ok(self.get_block_header()?.try_into()?)
    }

    pub fn get_starknet_version(&self) -> ReexecutionResult<StarknetVersion> {
        let raw_version: String = serde_json::from_value(
            retry_request!(self.retry_config, || {
                self.rpc_state_reader.send_rpc_request(
                    "starknet_getBlockWithTxHashes",
                    GetBlockWithTxHashesParams { block_id: self.rpc_state_reader.block_id },
                )
            })?["starknet_version"]
                .clone(),
        )?;
        Ok(StarknetVersion::try_from(raw_version.as_str())?)
    }

    /// Get all transaction hashes in the current block.
    pub fn get_tx_hashes(&self) -> ReexecutionResult<Vec<String>> {
        let raw_tx_hashes = serde_json::from_value(
            retry_request!(self.retry_config, || {
                self.rpc_state_reader.send_rpc_request(
                    "starknet_getBlockWithTxHashes",
                    &GetBlockWithTxHashesParams { block_id: self.rpc_state_reader.block_id },
                )
            })?["transactions"]
                .clone(),
        )?;
        Ok(serde_json::from_value(raw_tx_hashes)?)
    }

    pub fn get_tx_by_hash(&self, tx_hash: &str) -> ReexecutionResult<Transaction> {
        Ok(deserialize_transaction_json_to_starknet_api_tx(retry_request!(
            self.retry_config,
            || {
                self.rpc_state_reader.send_rpc_request(
                    "starknet_getTransactionByHash",
                    GetTransactionByHashParams { transaction_hash: tx_hash.to_string() },
                )
            }
        )?)?)
    }

    pub fn get_all_txs_in_block(&self) -> ReexecutionResult<Vec<(Transaction, TransactionHash)>> {
        // TODO(Aviv): Use batch request to get all txs in a block.
        self.get_tx_hashes()?
            .iter()
            .map(|tx_hash| match self.get_tx_by_hash(tx_hash) {
                Err(error) => Err(error),
                Ok(tx) => Ok((tx, TransactionHash(Felt::from_hex_unchecked(tx_hash)))),
            })
            .collect::<Result<_, _>>()
    }

    pub fn get_versioned_constants(&self) -> ReexecutionResult<&'static VersionedConstants> {
        Ok(VersionedConstants::get(&self.get_starknet_version()?)?)
    }

    pub fn get_block_context(&self) -> ReexecutionResult<BlockContext> {
        Ok(BlockContext::new(
            self.get_block_info()?,
            get_chain_info(&self.chain_id),
            self.get_versioned_constants()?.clone(),
            BouncerConfig::max(),
        ))
    }

    pub fn get_transaction_executor(
        self,
        block_context_next_block: BlockContext,
        transaction_executor_config: Option<TransactionExecutorConfig>,
        contract_class_manager: &ContractClassManager,
    ) -> ReexecutionResult<TransactionExecutor<StateReaderAndContractManager<RpcStateReader>>> {
        let old_block_number = BlockNumber(
            block_context_next_block.block_info().block_number.0
                - constants::STORED_BLOCK_HASH_BUFFER,
        );
        let old_block_hash = self.get_old_block_hash(old_block_number)?;
        // We don't collect class cache metrics for the reexecution.
        let class_cache_metrics = None;
        let state_reader_and_contract_manager = StateReaderAndContractManager::new(
            self,
            contract_class_manager.clone(),
            class_cache_metrics,
        );
        Ok(TransactionExecutor::<StateReaderAndContractManager<RpcStateReader>>::pre_process_and_create(
            state_reader_and_contract_manager,
            block_context_next_block,
            Some(BlockHashAndNumber { number: old_block_number, hash: old_block_hash }),
            transaction_executor_config.unwrap_or_default(),
        )?)
    }

    pub fn get_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff> {
        let raw_statediff =
            &retry_request!(self.retry_config, || self.rpc_state_reader.send_rpc_request(
                "starknet_getStateUpdate",
                GetBlockWithTxHashesParams { block_id: self.rpc_state_reader.block_id }
            ))?["state_diff"];

        let deployed_contracts = hashmap_from_raw::<ContractAddress, ClassHash>(
            raw_statediff,
            "deployed_contracts",
            "address",
            "class_hash",
        )?;
        let storage_diffs = nested_hashmap_from_raw::<ContractAddress, StorageKey, Felt>(
            raw_statediff,
            "storage_diffs",
            "address",
            "storage_entries",
            "key",
            "value",
        )?;
        let declared_classes = hashmap_from_raw::<ClassHash, CompiledClassHash>(
            raw_statediff,
            "declared_classes",
            "class_hash",
            "compiled_class_hash",
        )?;
        let nonces = hashmap_from_raw::<ContractAddress, Nonce>(
            raw_statediff,
            "nonces",
            "contract_address",
            "nonce",
        )?;
        let replaced_classes = hashmap_from_raw::<ContractAddress, ClassHash>(
            raw_statediff,
            "replaced_classes",
            "contract_address",
            "class_hash",
        )?;
        // We expect the deployed_contracts and replaced_classes to have disjoint addresses.
        let address_to_class_hash = disjoint_hashmap_union(deployed_contracts, replaced_classes);
        Ok(CommitmentStateDiff {
            address_to_class_hash,
            address_to_nonce: nonces,
            storage_updates: storage_diffs,
            class_hash_to_compiled_class_hash: declared_classes,
        })
    }

    pub fn get_contract_class_mapping_dumper(&self) -> Option<StarknetContractClassMapping> {
        self.contract_class_mapping_dumper.lock().unwrap().clone()
    }
}

impl ReexecutionStateReader for RpcStateReader {
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<StarknetContractClass> {
        let params = json!({
            "block_id": self.rpc_state_reader.block_id,
            "class_hash": class_hash.0.to_hex_string(),
        });
        let raw_contract_class =
            match self.rpc_state_reader.send_rpc_request("starknet_getClass", params.clone()) {
                Err(RPCStateReaderError::ClassHashNotFound(_)) => {
                    return Err(StateError::UndeclaredClassHash(*class_hash));
                }
                other_result => other_result,
            }?;

        let contract_class: StarknetContractClass =
            serde_json::from_value(raw_contract_class).map_err(serde_err_to_state_err)?;
        // Create a binding to avoid value being dropped.
        let mut dumper_binding = self.contract_class_mapping_dumper.lock().unwrap();
        // If dumper exists, insert the contract class to the mapping.
        if let Some(contract_class_mapping_dumper) = dumper_binding.as_mut() {
            contract_class_mapping_dumper.insert(*class_hash, contract_class.clone());
        }
        Ok(contract_class)
    }

    fn get_old_block_hash(&self, old_block_number: BlockNumber) -> ReexecutionResult<BlockHash> {
        let block_id = BlockId::Number(old_block_number);
        let params = GetBlockWithTxHashesParams { block_id };
        let response =
            self.rpc_state_reader.send_rpc_request("starknet_getBlockWithTxHashes", params)?;
        let block_hash_raw: String = serde_json::from_value(response["block_hash"].clone())?;
        Ok(BlockHash(Felt::from_hex(&block_hash_raw).unwrap()))
    }
}

pub struct ConsecutiveRpcStateReaders {
    pub last_block_state_reader: RpcStateReader,
    pub next_block_state_reader: RpcStateReader,
    contract_class_manager: ContractClassManager,
}

impl ConsecutiveRpcStateReaders {
    pub fn new(
        last_constructed_block_number: BlockNumber,
        config: Option<RpcStateReaderConfig>,
        chain_id: ChainId,
        dump_mode: bool,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        let config = config.unwrap_or(get_rpc_state_reader_config());
        Self {
            last_block_state_reader: RpcStateReader::new(
                &config,
                chain_id.clone(),
                last_constructed_block_number,
                dump_mode,
            ),
            next_block_state_reader: RpcStateReader::new(
                &config,
                chain_id,
                last_constructed_block_number.next().expect("Overflow in block number"),
                dump_mode,
            ),
            contract_class_manager,
        }
    }

    pub fn get_serializable_data_next_block(&self) -> ReexecutionResult<SerializableDataNextBlock> {
        let (transactions_next_block, declared_classes) =
            self.get_next_block_starknet_api_txs_and_declared_classes()?;
        assert_matches!(self.get_next_block_txs(), Ok(_));
        Ok(SerializableDataNextBlock {
            block_info_next_block: self.next_block_state_reader.get_block_info()?,
            starknet_version: self.next_block_state_reader.get_starknet_version()?,
            transactions_next_block,
            state_diff_next_block: self.next_block_state_reader.get_state_diff()?,
            declared_classes,
        })
    }

    pub fn get_old_block_hash(&self) -> ReexecutionResult<BlockHash> {
        self.last_block_state_reader.get_old_block_hash(BlockNumber(
            self.next_block_state_reader.get_block_context()?.block_info().block_number.0
                - constants::STORED_BLOCK_HASH_BUFFER,
        ))
    }

    fn get_next_block_starknet_api_txs_and_declared_classes(
        &self,
    ) -> ReexecutionResult<(Vec<(Transaction, TransactionHash)>, StarknetContractClassMapping)>
    {
        let transactions_next_block = self.next_block_state_reader.get_all_txs_in_block()?;
        self.next_block_state_reader
            .api_txs_to_blockifier_txs_next_block(transactions_next_block.clone())?;
        Ok((
            transactions_next_block,
            self.next_block_state_reader.get_contract_class_mapping_dumper().ok_or(
                StateError::StateReadError("Contract class mapping dumper is None.".to_string()),
            )?,
        ))
    }

    /// Executes a single transaction.
    /// Returns the execution output, initial reads and the block context.
    pub fn execute_single_api_tx(
        self,
        tx: Transaction,
    ) -> ReexecutionResult<(TransactionExecutionOutput, StateMaps, BlockContext)> {
        let chain_id = self.next_block_state_reader.chain_id.clone();
        let transaction_hash = tx.calculate_transaction_hash(&chain_id)?;

        let blockifier_txs = self
            .next_block_state_reader
            .api_txs_to_blockifier_txs_next_block(vec![(tx, transaction_hash)])?;
        let blockifier_tx = blockifier_txs
            .first()
            .expect("API to Blockifier transaction conversion returned empty list");

        let mut transaction_executor = self.pre_process_and_create_executor(None)?;
        let block_context = transaction_executor.block_context.as_ref().clone();

        let (tx_execution_info, state_diff) = transaction_executor.execute(blockifier_tx)?;
        let initial_reads =
            transaction_executor.block_state.as_ref().unwrap().get_initial_reads()?;
        Ok(((tx_execution_info, state_diff), initial_reads, block_context))
    }

    /// Executes a single transaction from a JSON file or given a transaction hash, using RPC to
    /// fetch block context. Does not assert correctness, only prints the execution result.
    pub fn execute_single_transaction(self, tx_input: TransactionInput) -> ReexecutionResult<()> {
        // Get transaction and hash based on input method.
        let (transaction, _) = match tx_input {
            TransactionInput::FromHash { tx_hash } => {
                // Fetch transaction from the next block (the block containing the transaction to
                // execute).
                let transaction = self.next_block_state_reader.get_tx_by_hash(&tx_hash)?;
                let transaction_hash = TransactionHash(Felt::from_hex_unchecked(&tx_hash));

                (transaction, transaction_hash)
            }
            TransactionInput::FromFile { tx_path } => {
                // Load the transaction from a local JSON file.
                let json_content = read_to_string(&tx_path).unwrap_or_else(|_| {
                    panic!("Failed to read transaction JSON file: {}.", tx_path)
                });
                let chain_id = self.next_block_state_reader.chain_id.clone();

                let json_value: Value = serde_json::from_str(&json_content)?;
                let transaction = deserialize_transaction_json_to_starknet_api_tx(json_value)?;
                let transaction_hash = transaction.calculate_transaction_hash(&chain_id)?;

                (transaction, transaction_hash)
            }
        };

        let (res, _, _) = self.execute_single_api_tx(transaction)?;

        println!("Execution result: {:?}", res);

        Ok(())
    }

    /// Writes the reexecution data required to reexecute a block to a JSON file.
    pub fn write_block_reexecution_data_to_file(self, full_file_path: &str) {
        let chain_id = self.next_block_state_reader.chain_id.clone();
        let block_number = self.next_block_state_reader.get_block_info().unwrap().block_number;

        let serializable_data_next_block = self.get_serializable_data_next_block().unwrap();

        let old_block_hash = self.get_old_block_hash().unwrap();

        // Run the reexecution and get the state maps and contract class mapping.
        let (block_state, expected_state_diff, actual_state_diff) = self.reexecute_block();

        // Warn if state diffs don't match, but continue writing the file.
        let expected_comparable = ComparableStateDiff::from(expected_state_diff);
        let actual_comparable = ComparableStateDiff::from(actual_state_diff);
        if expected_comparable != actual_comparable {
            println!(
                "WARNING: State diff mismatch for block {block_number}. Expected and actual state \
                 diffs do not match."
            );
        }

        let block_state = block_state.unwrap();
        let serializable_data_prev_block = SerializableDataPrevBlock {
            state_maps: block_state.get_initial_reads().unwrap(),
            contract_class_mapping: block_state
                .state
                .state_reader
                .get_contract_class_mapping_dumper()
                .unwrap(),
        };

        // Write the reexecution data to a json file.
        SerializableOfflineReexecutionData {
            serializable_data_prev_block,
            serializable_data_next_block,
            chain_id,
            old_block_hash,
        }
        .write_to_file(full_file_path)
        .unwrap();

        println!("RPC replies required for reexecuting block {block_number} written to json file.");
    }
}

impl ConsecutiveReexecutionStateReaders<StateReaderAndContractManager<RpcStateReader>>
    for ConsecutiveRpcStateReaders
{
    fn pre_process_and_create_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<StateReaderAndContractManager<RpcStateReader>>> {
        self.last_block_state_reader.get_transaction_executor(
            self.next_block_state_reader.get_block_context()?,
            transaction_executor_config,
            &self.contract_class_manager,
        )
    }

    fn get_next_block_txs(&self) -> ReexecutionResult<Vec<BlockifierTransaction>> {
        self.next_block_state_reader.api_txs_to_blockifier_txs_next_block(
            self.next_block_state_reader.get_all_txs_in_block()?,
        )
    }

    fn get_next_block_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff> {
        self.next_block_state_reader.get_state_diff()
    }
}
