use blockifier::blockifier::block::BlockInfo;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::ContractClass;
use blockifier::state::cached_state::CachedState;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::versioned_constants::{StarknetVersion, VersionedConstants};
use serde_json::json;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_core::types::ContractClass::{Legacy, Sierra};
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_gateway::errors::serde_err_to_state_err;
use starknet_gateway::rpc_objects::GetBlockWithTxHashesParams;
use starknet_gateway::rpc_state_reader::RpcStateReader;
use starknet_gateway::state_reader::MempoolStateReader;
use starknet_types_core::felt::Felt;

use crate::state_reader::compile::{legacy_to_contract_class_v0, sierra_to_contact_class_v1};
use crate::state_reader::utils::{get_chain_info, get_rpc_state_reader_config};

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
    fn get_compiled_contract_class(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
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

    pub fn get_block_info(&self) -> StateResult<BlockInfo> {
        self.0.get_block_info()
    }

    pub fn get_starknet_version(&self) -> StateResult<StarknetVersion> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let raw_version: String = serde_json::from_value(
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?
                ["starknet_version"]
                .clone(),
        )
        .map_err(serde_err_to_state_err)?;
        // Use the TryFrom implementation to convert the version bytes to a StarknetVersion
        StarknetVersion::try_from(raw_version.as_str()).map_err(|err| {
            StateError::StateReadError(format!("Failed to match starknet version: {}", err))
        })
    }

    pub fn get_txs_hash(&self) -> StateResult<Vec<String>> {
        let get_block_params = GetBlockWithTxHashesParams { block_id: self.0.block_id };
        let raw_tx_hash = serde_json::from_value(
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", &get_block_params)?
                ["transactions"]
                .clone(),
        )
        .map_err(serde_err_to_state_err)?;
        serde_json::from_value(raw_tx_hash).map_err(serde_err_to_state_err)
    }

    pub fn get_txs_by_hash(&self, tx_hash: &str) -> StateResult<String> {
        let method = "starknet_getTransactionByHash";
        let params = json!({
            "transaction_hash": tx_hash,
        });
        serde_json::from_value(self.0.send_rpc_request(method, params)?)
            .map_err(serde_err_to_state_err)
    }

    pub fn get_contract_class(
        &self,
        class_hash: &ClassHash,
    ) -> StateResult<starknet_core::types::ContractClass> {
        let params = json!({
        "block_id": self.0.block_id,
        "class_hash": class_hash.0.to_string(),
        });
        let contract_class: starknet_core::types::ContractClass =
            serde_json::from_value(self.0.send_rpc_request("starknet_getClass", params.clone())?)
                .map_err(serde_err_to_state_err)?;
        Ok(contract_class)
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
