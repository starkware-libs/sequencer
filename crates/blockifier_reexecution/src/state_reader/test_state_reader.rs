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
use serde_json::{json, to_value};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_api::transaction::{Transaction, TransactionHash};
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

    pub fn get_all_txs_in_block(&self) -> StateResult<Vec<(Transaction, TransactionHash)>> {
        // TODO(Aviv): Use batch request to get all txs in a block.
        self.get_tx_hashes()?
            .iter()
            .map(|tx_hash| match self.get_tx_by_hash(tx_hash) {
                Err(error) => Err(error),
                Ok(tx) => Ok((tx, TransactionHash(Felt::from_hex_unchecked(tx_hash)))),
            })
            .collect::<Result<_, _>>()
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

    pub fn get_transaction_executor(
        test_state_reader: TestStateReader,
    ) -> StateResult<TransactionExecutor<TestStateReader>> {
        let block_context = test_state_reader.get_block_context()?;
        Ok(TransactionExecutor::<TestStateReader>::new(
            CachedState::new(test_state_reader),
            block_context,
            TransactionExecutorConfig::default(),
        ))
    }
}
