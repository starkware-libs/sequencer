use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use apollo_gateway::config::RpcStateReaderConfig;
use apollo_gateway::errors::{serde_err_to_state_err, RPCStateReaderError};
use apollo_gateway::rpc_objects::{BlockHeader, BlockId, GetBlockWithTxHashesParams};
use apollo_gateway::rpc_state_reader::RpcStateReader;
use assert_matches::assert_matches;
use blockifier::abi::constants;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use serde::Serialize;
use serde_json::{json, to_value};
use starknet_api::block::{
    BlockHash,
    BlockHashAndNumber,
    BlockInfo,
    BlockNumber,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{CommitmentStateDiff, StorageKey};
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_core::types::ContractClass as StarknetContractClass;
use starknet_types_core::felt::Felt;

use crate::retry_request;
use crate::state_reader::compile::{
    legacy_to_contract_class_v0,
    sierra_to_versioned_contract_class_v1,
};
use crate::state_reader::errors::ReexecutionResult;
use crate::state_reader::offline_state_reader::SerializableDataNextBlock;
use crate::state_reader::reexecution_state_reader::{
    ConsecutiveReexecutionStateReaders,
    ReexecutionStateReader,
};
use crate::state_reader::serde_utils::{
    deserialize_transaction_json_to_starknet_api_tx,
    hashmap_from_raw,
    nested_hashmap_from_raw,
};
use crate::state_reader::utils::{
    disjoint_hashmap_union,
    get_chain_info,
    get_rpc_state_reader_config,
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
pub struct TestStateReader {
    pub(crate) rpc_state_reader: RpcStateReader,
    pub(crate) retry_config: RetryConfig,
    pub(crate) chain_id: ChainId,
    #[allow(dead_code)]
    pub(crate) contract_class_mapping_dumper: Arc<Mutex<Option<StarknetContractClassMapping>>>,
}

impl Default for TestStateReader {
    fn default() -> Self {
        Self {
            rpc_state_reader: RpcStateReader::from_latest(&get_rpc_state_reader_config()),
            retry_config: RetryConfig::default(),
            chain_id: ChainId::Mainnet,
            contract_class_mapping_dumper: Arc::new(Mutex::new(None)),
        }
    }
}

impl StateReader for TestStateReader {
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
                let (casm, _) = sierra_to_versioned_contract_class_v1(sierra).unwrap();
                Ok(RunnableCompiledClass::try_from(casm).unwrap())
            }
            StarknetContractClass::Legacy(legacy) => {
                Ok(legacy_to_contract_class_v0(legacy).unwrap().try_into().unwrap())
            }
        }
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.rpc_state_reader.get_compiled_class_hash(class_hash)
    }
}

impl TestStateReader {
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
            rpc_state_reader: RpcStateReader::from_number(config, block_number),
            retry_config: RetryConfig::default(),
            chain_id,
            contract_class_mapping_dumper,
        }
    }

    pub fn new_for_testing(block_number: BlockNumber) -> Self {
        TestStateReader::new(&get_rpc_state_reader_config(), ChainId::Mainnet, block_number, false)
    }

    /// Get the block info of the current block.
    /// If l2_gas_price is not present in the block header, it will be set to 1.
    pub fn get_block_info(&self) -> ReexecutionResult<BlockInfo> {
        let mut json = retry_request!(self.retry_config, || {
            self.rpc_state_reader.send_rpc_request(
                "starknet_getBlockWithTxHashes",
                GetBlockWithTxHashesParams { block_id: self.rpc_state_reader.block_id },
            )
        })?;

        let block_header_map = json.as_object_mut().ok_or(StateError::StateReadError(
            "starknet_getBlockWithTxHashes should return JSON value of type Object".to_string(),
        ))?;

        if block_header_map.get("l2_gas_price").is_none() {
            // In old blocks, the l2_gas_price field is not present.
            block_header_map.insert(
                "l2_gas_price".to_string(),
                to_value(GasPricePerToken {
                    price_in_wei: 1_u8.into(),
                    price_in_fri: 1_u8.into(),
                })?,
            );
        }

        Ok(serde_json::from_value::<BlockHeader>(json)?.try_into()?)
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
    ) -> ReexecutionResult<TransactionExecutor<TestStateReader>> {
        let old_block_number = BlockNumber(
            block_context_next_block.block_info().block_number.0
                - constants::STORED_BLOCK_HASH_BUFFER,
        );
        let old_block_hash = self.get_old_block_hash(old_block_number)?;
        Ok(TransactionExecutor::<TestStateReader>::pre_process_and_create(
            self,
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

impl ReexecutionStateReader for TestStateReader {
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

pub struct ConsecutiveTestStateReaders {
    pub last_block_state_reader: TestStateReader,
    pub next_block_state_reader: TestStateReader,
}

impl ConsecutiveTestStateReaders {
    pub fn new(
        last_constructed_block_number: BlockNumber,
        config: Option<RpcStateReaderConfig>,
        chain_id: ChainId,
        dump_mode: bool,
    ) -> Self {
        let config = config.unwrap_or(get_rpc_state_reader_config());
        Self {
            last_block_state_reader: TestStateReader::new(
                &config,
                chain_id.clone(),
                last_constructed_block_number,
                dump_mode,
            ),
            next_block_state_reader: TestStateReader::new(
                &config,
                chain_id,
                last_constructed_block_number.next().expect("Overflow in block number"),
                dump_mode,
            ),
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
}

impl ConsecutiveReexecutionStateReaders<TestStateReader> for ConsecutiveTestStateReaders {
    fn pre_process_and_create_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<TestStateReader>> {
        self.last_block_state_reader.get_transaction_executor(
            self.next_block_state_reader.get_block_context()?,
            transaction_executor_config,
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
