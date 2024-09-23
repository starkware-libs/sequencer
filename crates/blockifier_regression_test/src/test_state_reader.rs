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
use starknet_api::block::{BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_gateway::errors::serde_err_to_state_err;
use starknet_gateway::rpc_objects::GetBlockWithTxHashesParams;
use starknet_gateway::rpc_state_reader::RpcStateReader;
use starknet_gateway::state_reader::MempoolStateReader;
use starknet_types_core::felt::Felt;

use crate::test_utils::{get_chain_info, get_rpc_state_reader_config};

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

    fn get_compiled_contract_class(&self, _class_hash: ClassHash) -> StateResult<ContractClass> {
        todo!()
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
        let block_header: BlockHeader = serde_json::from_value(
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?,
        )
        .map_err(serde_err_to_state_err)?;
        let raw_version = block_header.starknet_version.0.as_slice();
        match raw_version {
            [0, 13, 0] => Ok(StarknetVersion::V0_13_0),
            [0, 13, 1] => Ok(StarknetVersion::V0_13_1),
            [0, 13, 1, 1] => Ok(StarknetVersion::V0_13_1_1),
            [0, 13, 2] => Ok(StarknetVersion::V0_13_2),
            [0, 13, 2, 1] => Ok(StarknetVersion::V0_13_2_1),
            _ => Err(StateError::StateReadError("Failed to match starknet version".to_string())),
        }
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
