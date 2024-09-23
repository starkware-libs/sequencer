use blockifier::blockifier::block::BlockInfo;
use blockifier::execution::contract_class::ContractClass;
use blockifier::state::state_api::{StateReader, StateResult};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_gateway::config::RpcStateReaderConfig;
use starknet_gateway::rpc_state_reader::RpcStateReader;
use starknet_types_core::felt::Felt;
use starknet_gateway::state_reader::MempoolStateReader;

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

    pub fn get_block_info(&self) -> StateResult<BlockInfo> {
        self.0.get_block_info()
    }

}
