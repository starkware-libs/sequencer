use blockifier::blockifier::block::BlockInfo;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::ContractClass as BlockifierContractClass;
use blockifier::state::cached_state::CachedState;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::versioned_constants::{StarknetVersion, VersionedConstants};
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{json, to_value, Value};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, StateDiff, StorageKey};
use starknet_api::transaction::Transaction;
use starknet_core::types::ContractClass as StarknetContractClass;
use starknet_core::types::ContractClass::{Legacy, Sierra};
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_gateway::errors::serde_err_to_state_err;
use starknet_gateway::rpc_objects::{BlockHeader, GetBlockWithTxHashesParams, ResourcePrice};
use starknet_gateway::rpc_state_reader::RpcStateReader;
use starknet_types_core::felt::Felt;

use crate::state_reader::compile::{legacy_to_contract_class_v0, sierra_to_contact_class_v1};
use crate::state_reader::utils::{
    deserialize_transaction_json_to_starknet_api_tx,
    get_chain_info,
    get_rpc_state_reader_config,
};

pub struct TestStateReader(RpcStateReader);

impl StateReader for TestStateReader {
    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.0.get_nonce_at(contract_address)
    }

    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        self.0.get_storage_at(contract_address, key)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.0.get_class_hash_at(contract_address)
    }

    /// Returns the contract class of the given class hash.
    /// Compile the contract class if it is Sierra.
    fn get_compiled_contract_class(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<BlockifierContractClass> {
        match self.get_contract_class(&class_hash)? {
            Sierra(sierra) => sierra_to_contact_class_v1(sierra),
            Legacy(legacy) => legacy_to_contract_class_v0(legacy),
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
    pub fn get_block_info(&self) -> StateResult<BlockInfo> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let default_l2_price =
            ResourcePrice { price_in_wei: 1_u8.into(), price_in_fri: 1_u8.into() };

        let mut json =
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?;

        let block_header_map = json.as_object_mut().ok_or(StateError::StateReadError(
            "starknet_getBlockWithTxHashes should return JSON value of type Object".to_string(),
        ))?;

        if block_header_map.get("l2_gas_price").is_none() {
            // In old blocks, the l2_gas_price field is not present.
            block_header_map.insert(
                "l2_gas_price".to_string(),
                to_value(default_l2_price).map_err(serde_err_to_state_err)?,
            );
        }

        Ok(serde_json::from_value::<BlockHeader>(json)
            .map_err(serde_err_to_state_err)?
            .try_into()?)
    }

    pub fn get_starknet_version(&self) -> StateResult<StarknetVersion> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let raw_version: String = serde_json::from_value(
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?
                ["starknet_version"]
                .clone(),
        )
        .map_err(serde_err_to_state_err)?;
        StarknetVersion::try_from(raw_version.as_str()).map_err(|err| {
            StateError::StateReadError(format!("Failed to match starknet version: {}", err))
        })
    }

    /// Get all transaction hashes in the current block.
    pub fn get_tx_hashes(&self) -> StateResult<Vec<String>> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let raw_tx_hashes = serde_json::from_value(
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", &get_block_params)?
                ["transactions"]
                .clone(),
        )
        .map_err(serde_err_to_state_err)?;
        serde_json::from_value(raw_tx_hashes).map_err(serde_err_to_state_err)
    }

    pub fn get_tx_by_hash(&self, tx_hash: &str) -> StateResult<Transaction> {
        let method = "starknet_getTransactionByHash";
        let params = json!({
            "transaction_hash": tx_hash,
        });
        deserialize_transaction_json_to_starknet_api_tx(self.0.send_rpc_request(method, params)?)
            .map_err(serde_err_to_state_err)
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

    pub fn get_all_txs_in_block(&self) -> StateResult<Vec<Transaction>> {
        // TODO(Aviv): Use batch request to get all txs in a block.
        let txs: Vec<_> = self
            .get_tx_hashes()?
            .iter()
            .map(|tx_hash| self.get_tx_by_hash(tx_hash))
            .collect::<Result<_, _>>()?;
        Ok(txs)
    }

    pub fn get_versioned_constants(&self) -> StateResult<&'static VersionedConstants> {
        Ok(self.get_starknet_version()?.into())
    }

    pub fn get_block_context(&self) -> StateResult<BlockContext> {
        Ok(BlockContext::new(
            self.get_block_info()?,
            get_chain_info(),
            self.get_versioned_constants()?.clone(),
            BouncerConfig::max(),
        ))
    }

    pub fn get_transaction_executor(self) -> StateResult<TransactionExecutor<TestStateReader>> {
        let block_context = self.get_block_context()?;
        Ok(TransactionExecutor::<TestStateReader>::new(
            CachedState::new(self),
            block_context,
            TransactionExecutorConfig::default(),
        ))
    }

    pub fn get_state_diff(self) -> StateResult<StateDiff> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let raw_statediff =
            &self.0.send_rpc_request("starknet_getStateUpdate", get_block_params)?["state_diff"];
        Ok(StateDiff {
            deployed_contracts: hashmap_from_raw::<ContractAddress, ClassHash>(
                raw_statediff,
                "deployed_contracts",
                "address",
                "class_hash",
            )?,
            storage_diffs: nested_hashmap_from_raw::<ContractAddress, StorageKey, Felt>(
                raw_statediff,
                "storage_diffs",
                "address",
                "storage_entries",
                "key",
                "value",
            )?,
            declared_classes: hashmap_from_raw::<ClassHash, (CompiledClassHash, ContractClass)>(
                raw_statediff,
                "declared_classes",
                "class_hash",
                "compiled_class_hash",
            )?,
            deprecated_declared_classes: hashmap_from_raw::<ClassHash, DeprecatedContractClass>(
                raw_statediff,
                "deprecated_declared_classes",
                "", // TODO (need non-empty example)
                "", // TODO (need non-empty example)
            )?,
            nonces: hashmap_from_raw::<ContractAddress, Nonce>(
                raw_statediff,
                "nonces",
                "contract_address",
                "nonce",
            )?,
            replaced_classes: hashmap_from_raw::<ContractAddress, ClassHash>(
                raw_statediff,
                "replaced_classes",
                "class_hash",
                "contract_address",
            )?,
        })
    }
}

fn hashmap_from_raw<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    V: for<'de> Deserialize<'de>,
>(
    raw_object: &Value,
    vec_str: &str,
    key_str: &str,
    value_str: &str,
) -> StateResult<IndexMap<K, V>> {
    Ok(vec_to_hashmap::<K, V>(
        serde_json::from_value(raw_object[vec_str].clone()).map_err(serde_err_to_state_err)?,
        key_str,
        value_str,
    ))
}

fn nested_hashmap_from_raw<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VK: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VV: for<'de> Deserialize<'de>,
>(
    raw_object: &Value,
    vec_str: &str,
    key_str: &str,
    value_str: &str,
    inner_key_str: &str,
    inner_value_str: &str,
) -> StateResult<IndexMap<K, IndexMap<VK, VV>>> {
    Ok(vec_to_nested_hashmap::<K, VK, VV>(
        serde_json::from_value(raw_object[vec_str].clone()).map_err(serde_err_to_state_err)?,
        key_str,
        value_str,
        inner_key_str,
        inner_value_str,
    ))
}

fn vec_to_hashmap<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    V: for<'de> Deserialize<'de>,
>(
    vec: Vec<Value>,
    key_str: &str,
    value_str: &str,
) -> IndexMap<K, V> {
    vec.iter()
        .map(|element| {
            (
                serde_json::from_value(element[key_str].clone())
                    .expect("Key string doesn't match expected."),
                serde_json::from_value(element[value_str].clone())
                    .expect("Value string doesn't match expected."),
            )
        })
        .collect()
}

fn vec_to_nested_hashmap<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VK: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VV: for<'de> Deserialize<'de>,
>(
    vec: Vec<Value>,
    key_str: &str,
    value_str: &str,
    inner_key_str: &str,
    inner_value_str: &str,
) -> IndexMap<K, IndexMap<VK, VV>> {
    vec.iter()
        .map(|element| {
            (
                serde_json::from_value(element[key_str].clone()).expect("Couldn't deserialize key"),
                vec_to_hashmap(
                    serde_json::from_value(element[value_str].clone())
                        .expect("Couldn't deserialize value"),
                    inner_key_str,
                    inner_value_str,
                ),
            )
        })
        .collect()
}
