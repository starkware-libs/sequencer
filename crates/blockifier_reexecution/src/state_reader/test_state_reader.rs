use blockifier::blockifier::block::BlockInfo;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::{ClassInfo, ContractClass as BlockifierContractClass};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier::versioned_constants::VersionedConstants;
use papyrus_execution::DEPRECATED_CONTRACT_SIERRA_SIZE;
use retry::delay::Fixed;
use retry::retry;
use serde_json::{json, to_value};
use starknet_api::block::{BlockNumber, StarknetVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_core::types::ContractClass as StarknetContractClass;
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_gateway::errors::serde_err_to_state_err;
use starknet_gateway::rpc_objects::{BlockHeader, GetBlockWithTxHashesParams, ResourcePrice};
use starknet_gateway::rpc_state_reader::RpcStateReader;
use starknet_types_core::felt::Felt;

use crate::state_reader::compile::{legacy_to_contract_class_v0, sierra_to_contact_class_v1};
use crate::state_reader::errors::ReexecutionError;
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

pub type ReexecutionResult<T> = Result<T, ReexecutionError>;

pub struct TestStateReader(RpcStateReader);

impl StateReader for TestStateReader {
    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        retry(Fixed::from_millis(100).take(3), || {
            match self.0.get_nonce_at(contract_address) {
                Ok(value) => Ok(value),
                // If the error is a connection error, we want to retry.
                Err(e) if e.to_string().contains("connection error") => Err(e),
                Err(e) => panic!("Unexpected error in get_nonce_at: {:?}", e),
            }
        })
        .map_err(|e| e.error)
    }

    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        retry(Fixed::from_millis(100).take(3), || {
            match self.0.get_storage_at(contract_address, key) {
                Ok(value) => Ok(value),
                Err(e) if e.to_string().contains("Contract address not found for request") => {
                    Ok(Felt::default())
                }
                // If the error is a connection error, we want to retry.
                Err(e) if e.to_string().contains("connection error") => Err(e),
                Err(e) => panic!("Unexpected error in get_storage_at: {:?}", e),
            }
        })
        .map_err(|e| e.error)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        retry(Fixed::from_millis(100).take(3), || {
            match self.0.get_class_hash_at(contract_address) {
                Ok(value) => Ok(value),
                // If the error is a connection error, we want to retry.
                Err(e) if e.to_string().contains("connection error") => Err(e),
                Err(e) => panic!("Unexpected error in get_class_hash_at: {:?}", e),
            }
        })
        .map_err(|e| e.error)
    }

    /// Returns the contract class of the given class hash.
    /// Compile the contract class if it is Sierra.
    fn get_compiled_contract_class(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<BlockifierContractClass> {
        match self.get_contract_class(&class_hash)? {
            StarknetContractClass::Sierra(sierra) => sierra_to_contact_class_v1(sierra),
            StarknetContractClass::Legacy(legacy) => legacy_to_contract_class_v0(legacy),
        }
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.0.get_compiled_class_hash(class_hash)
    }
}

impl TestStateReader {
    pub fn new(config: &RpcStateReaderConfig, block_number: BlockNumber) -> Self {
        Self(RpcStateReader::from_number(config, block_number))
    }

    pub fn new_for_testing(block_number: BlockNumber) -> Self {
        TestStateReader::new(&get_rpc_state_reader_config(), block_number)
    }

    /// Get the block info of the current block.
    /// If l2_gas_price is not present in the block header, it will be set to 1.
    pub fn get_block_info(&self) -> ReexecutionResult<BlockInfo> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };

        let mut json =
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?;

        let block_header_map = json.as_object_mut().ok_or(StateError::StateReadError(
            "starknet_getBlockWithTxHashes should return JSON value of type Object".to_string(),
        ))?;

        if block_header_map.get("l2_gas_price").is_none() {
            // In old blocks, the l2_gas_price field is not present.
            block_header_map.insert(
                "l2_gas_price".to_string(),
                to_value(ResourcePrice { price_in_wei: 1_u8.into(), price_in_fri: 1_u8.into() })?,
            );
        }

        Ok(serde_json::from_value::<BlockHeader>(json)?.try_into()?)
    }

    pub fn get_starknet_version(&self) -> ReexecutionResult<StarknetVersion> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let raw_version: String = serde_json::from_value(
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?
                ["starknet_version"]
                .clone(),
        )?;
        Ok(StarknetVersion::try_from(raw_version.as_str())?)
    }

    /// Get all transaction hashes in the current block.
    pub fn get_tx_hashes(&self) -> ReexecutionResult<Vec<String>> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let raw_tx_hashes = serde_json::from_value(
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", &get_block_params)?
                ["transactions"]
                .clone(),
        )?;
        Ok(serde_json::from_value(raw_tx_hashes)?)
    }

    pub fn get_tx_by_hash(&self, tx_hash: &str) -> ReexecutionResult<Transaction> {
        let method = "starknet_getTransactionByHash";
        let params = json!({
            "transaction_hash": tx_hash,
        });
        Ok(deserialize_transaction_json_to_starknet_api_tx(
            self.0.send_rpc_request(method, params)?,
        )?)
    }

    pub fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<StarknetContractClass> {
        let params = json!({
            "block_id": self.0.block_id,
            "class_hash": class_hash.0.to_string(),
        });
        let contract_class: StarknetContractClass =
            serde_json::from_value(self.0.send_rpc_request("starknet_getClass", params.clone())?)
                .map_err(serde_err_to_state_err)?;
        Ok(contract_class)
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
            get_chain_info(),
            self.get_versioned_constants()?.clone(),
            BouncerConfig::max(),
        ))
    }

    pub fn get_transaction_executor(
        self,
        block_context_next_block: BlockContext,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<TestStateReader>> {
        Ok(TransactionExecutor::<TestStateReader>::new(
            CachedState::new(self),
            block_context_next_block,
            transaction_executor_config.unwrap_or_default(),
        ))
    }

    pub fn get_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let raw_statediff =
            &self.0.send_rpc_request("starknet_getStateUpdate", get_block_params)?["state_diff"];
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
            "class_hash",
            "contract_address",
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

    pub fn get_class_info(&self, class_hash: ClassHash) -> ReexecutionResult<ClassInfo> {
        match self.get_contract_class(&class_hash)? {
            StarknetContractClass::Sierra(sierra) => {
                let abi_length = sierra.abi.len();
                let sierra_length = sierra.sierra_program.len();
                Ok(ClassInfo::new(&sierra_to_contact_class_v1(sierra)?, sierra_length, abi_length)?)
            }
            StarknetContractClass::Legacy(legacy) => {
                let abi_length =
                    legacy.abi.clone().expect("legendary contract should have abi").len();
                Ok(ClassInfo::new(
                    &legacy_to_contract_class_v0(legacy)?,
                    DEPRECATED_CONTRACT_SIERRA_SIZE,
                    abi_length,
                )?)
            }
        }
    }

    // TODO(Aner): extend/refactor to accomodate all types of transactions.
    pub(crate) fn from_api_txs_to_blockifier_txs(
        self: &TestStateReader,
        txs_and_hashes: Vec<(Transaction, TransactionHash)>,
    ) -> ReexecutionResult<Vec<BlockifierTransaction>> {
        txs_and_hashes
            .into_iter()
            .map(|(tx, tx_hash)| match tx {
                Transaction::Invoke(_) | Transaction::DeployAccount(_) => {
                    BlockifierTransaction::from_api(tx, tx_hash, None, None, None, false)
                        .map_err(ReexecutionError::from)
                }
                Transaction::Declare(ref declare_tx) => {
                    let class_info = self
                        .get_class_info(declare_tx.class_hash())
                        .map_err(ReexecutionError::from)?;
                    BlockifierTransaction::from_api(
                        tx,
                        tx_hash,
                        Some(class_info),
                        None,
                        None,
                        false,
                    )
                    .map_err(ReexecutionError::from)
                }
                _ => unimplemented!("unimplemented transaction type: {:?}", tx),
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

/// Trait of the functions \ queries required for reexecution.
pub trait ConsecutiveStateReaders<S: StateReader> {
    fn get_transaction_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<S>>;

    fn get_next_block_txs(&self) -> ReexecutionResult<Vec<BlockifierTransaction>>;

    fn get_next_block_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff>;
}

pub struct ConsecutiveTestStateReaders {
    pub last_block_state_reader: TestStateReader,
    pub next_block_state_reader: TestStateReader,
}

impl ConsecutiveTestStateReaders {
    pub fn new(
        last_constructed_block_number: BlockNumber,
        config: Option<RpcStateReaderConfig>,
    ) -> Self {
        let config = config.unwrap_or(get_rpc_state_reader_config());
        ConsecutiveTestStateReaders {
            last_block_state_reader: TestStateReader::new(&config, last_constructed_block_number),
            next_block_state_reader: TestStateReader::new(
                &config,
                last_constructed_block_number.next().expect("Overflow in block number"),
            ),
        }
    }
}

impl ConsecutiveStateReaders<TestStateReader> for ConsecutiveTestStateReaders {
    fn get_transaction_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<TestStateReader>> {
        self.last_block_state_reader.get_transaction_executor(
            self.next_block_state_reader.get_block_context()?,
            transaction_executor_config,
        )
    }

    fn get_next_block_txs(&self) -> ReexecutionResult<Vec<BlockifierTransaction>> {
        self.next_block_state_reader
            .from_api_txs_to_blockifier_txs(self.next_block_state_reader.get_all_txs_in_block()?)
    }

    fn get_next_block_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff> {
        self.next_block_state_reader.get_state_diff()
    }
}
