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
use serde_json::to_value;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_gateway::errors::serde_err_to_state_err;
use starknet_gateway::rpc_objects::{BlockHeader, GetBlockWithTxHashesParams, ResourcePrice};
use starknet_gateway::rpc_state_reader::RpcStateReader;
use starknet_types_core::felt::Felt;

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

    fn get_compiled_contract_class(&self, _class_hash: ClassHash) -> StateResult<ContractClass> {
        todo!()
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
            ResourcePrice { price_in_wei: GasPrice(1), price_in_fri: GasPrice(1) };

        let mut json =
            self.0.send_rpc_request("starknet_getBlockWithTxHashes", get_block_params)?;

        // Convert the JSON response to an object and handle potential errors.
        let block_header_map = match json.as_object_mut() {
            Some(map) => map,
            None => {
                return Err(StateError::StateReadError(
                    "starknet_getBlockWithTxHashes should return JSON value of type Object"
                        .to_string(),
                ));
            }
        };

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
