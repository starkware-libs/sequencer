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
use starknet_api::transaction::Transaction;
use starknet_core::types::ContractClass::{Legacy, Sierra};
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_gateway::errors::serde_err_to_state_err;
use starknet_gateway::rpc_objects::GetBlockWithTxHashesParams;
use starknet_gateway::rpc_state_reader::RpcStateReader;
use starknet_gateway::state_reader::MempoolStateReader;
use starknet_types_core::felt::Felt;

use crate::compile::{legacy_to_contract_class_v0, sierra_to_contact_class_v1};
use crate::test_utils::{
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
    pub fn new(config: Option<&RpcStateReaderConfig>, block_number: BlockNumber) -> Self {
        match config {
            Some(config) => Self(RpcStateReader::from_number(config, block_number)),
            None => Self(RpcStateReader::from_number(&get_rpc_state_reader_config(), block_number)),
        }
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
        match raw_version.as_str() {
            "0.13.0" => Ok(StarknetVersion::V0_13_0),
            "0.13.1" => Ok(StarknetVersion::V0_13_1),
            "0.13.1.1" => Ok(StarknetVersion::V0_13_1_1),
            "0.13.2" => Ok(StarknetVersion::V0_13_2),
            "0.13.2.1" => Ok(StarknetVersion::V0_13_2_1),
            _ => Err(StateError::StateReadError("Failed to match starknet version".to_string())),
        }
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

    pub fn get_txs_by_hash(&self, tx_hash: &str) -> StateResult<Transaction> {
        let method = "starknet_getTransactionByHash";
        let params = json!({
            "transaction_hash": tx_hash,
        });
        deserialize_transaction_json_to_starknet_api_tx(self.0.send_rpc_request(method, params)?)
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

    pub fn get_all_txs_in_block(&self) -> StateResult<Vec<Transaction>> {
        let txs_hash = self.get_txs_hash()?;
        let mut txs = Vec::new();
        for tx_hash in txs_hash {
            txs.push(self.get_txs_by_hash(&tx_hash)?);
        }
        Ok(txs)
    }

    pub fn get_versioned_constants(&self) -> StateResult<&'static VersionedConstants> {
        let starknet_version = self.get_starknet_version()?;
        let versioned_constants: &'static VersionedConstants = starknet_version.into();
        Ok(versioned_constants)
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

#[cfg(test)]
pub mod test {
    use std::fs::File;

    use assert_matches::assert_matches;
    use blockifier::blockifier::block::BlockInfo;
    use blockifier::state::state_api::StateReader;
    use blockifier::versioned_constants::StarknetVersion;
    use rstest::*;
    use starknet_api::block::BlockNumber;
    use starknet_api::core::ClassHash;
    use starknet_api::transaction::Transaction;
    use starknet_api::{class_hash, felt};

    use super::TestStateReader;

    #[fixture]
    pub fn test_state_reader() -> TestStateReader {
        TestStateReader::new(None, BlockNumber(700000))
    }

    #[fixture]
    pub fn test_block_number() -> BlockNumber {
        BlockNumber(700000)
    }

    #[rstest]
    #[ignore = "This test using http request, so it should not be run in CI"]
    pub fn test_get_block_info(test_state_reader: TestStateReader, test_block_number: BlockNumber) {
        assert_matches!(test_state_reader.get_block_info() ,  Ok(BlockInfo{block_number, .. }) if block_number==test_block_number);
    }

    #[rstest]
    #[ignore = "This test using http request, so it should not be run in CI"]
    pub fn test_get_starknet_version(test_state_reader: TestStateReader) {
        assert!(test_state_reader.get_starknet_version().unwrap() == StarknetVersion::V0_13_2_1)
    }

    #[rstest]
    #[ignore = "This test uses an HTTP request, so it should not be run in CI."]
    pub fn test_get_contract_class(test_state_reader: TestStateReader) {
        let class_hash =
            class_hash!("0x3131fa018d520a037686ce3efddeab8f28895662f019ca3ca18a626650f7d1e");
        test_state_reader.get_contract_class(&class_hash).unwrap_or_else(|err| {
            panic!("Error retrieving contract class for class hash {}: {}", class_hash, err);
        });
    }

    #[rstest]
    #[ignore = "This test uses an HTTP request, so it should not be run in CI."]
    pub fn test_get_compiled_contract_class(test_state_reader: TestStateReader) {
        let class_hash =
            class_hash!("0x3131fa018d520a037686ce3efddeab8f28895662f019ca3ca18a626650f7d1e");
        test_state_reader.get_compiled_contract_class(class_hash).unwrap_or_else(|err| {
            panic!(
                "Error retrieving compiled contract class for class hash {}: {}",
                class_hash, err
            );
        });
    }

    #[rstest]
    #[ignore = "This test uses an HTTP request, so it should not be run in CI."]
    pub fn test_get_txs_hash(test_state_reader: TestStateReader) {
        let raw_txs_hash = File::open("./src/data/txs_hash_block_700000.json").unwrap();
        let txs_hash: Vec<String> = serde_json::from_reader(raw_txs_hash).unwrap();
        let actual_tx_hash = test_state_reader.get_txs_hash().unwrap();
        assert!(actual_tx_hash == txs_hash);
    }

    #[rstest]
    #[ignore = "This test uses an HTTP request, so it should not be run in CI."]
    pub fn test_get_tx_by_hash(test_state_reader: TestStateReader) {
        let tx_hash = "0x47165a9a9c97e8829a4778f2a4b6fae4366aefc35b51d484bf04c458168351b";
        let actual_tx = test_state_reader.get_txs_by_hash(tx_hash).unwrap();
        assert_matches!(actual_tx, Transaction::Invoke(..));
    }
}
