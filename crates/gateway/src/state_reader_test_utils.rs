use blockifier::blockifier::block::BlockInfo;
use blockifier::execution::contract_class::ContractClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use blockifier::test_utils::dict_state_reader::DictStateReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;

use crate::state_reader::{MempoolStateReader, StateReaderFactory};

#[derive(Clone)]
pub struct TestStateReader {
    pub block_info: BlockInfo,
    pub blockifier_state_reader: DictStateReader,
}

impl MempoolStateReader for TestStateReader {
    fn get_block_info(&self) -> Result<BlockInfo, StateError> {
        Ok(self.block_info.clone())
    }
}

impl BlockifierStateReader for TestStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        self.blockifier_state_reader.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.blockifier_state_reader.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.blockifier_state_reader.get_class_hash_at(contract_address)
    }

    fn get_compiled_contract_class(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
        self.blockifier_state_reader.get_compiled_contract_class(class_hash)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.blockifier_state_reader.get_compiled_class_hash(class_hash)
    }
}

pub struct TestStateReaderFactory {
    pub state_reader: TestStateReader,
}

impl StateReaderFactory for TestStateReaderFactory {
    fn get_state_reader_from_latest_block(&self) -> Box<dyn MempoolStateReader> {
        Box::new(self.state_reader.clone())
    }

    fn get_state_reader(&self, _block_number: BlockNumber) -> Box<dyn MempoolStateReader> {
        Box::new(self.state_reader.clone())
    }
}
