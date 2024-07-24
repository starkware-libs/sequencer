use blockifier::blockifier::block::BlockInfo;
use blockifier::execution::contract_class::ContractClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

pub trait MempoolStateReader: BlockifierStateReader + Send + Sync {
    fn get_block_info(&self) -> Result<BlockInfo, StateError>;
}

#[cfg_attr(test, automock)]
pub trait StateReaderFactory: Send + Sync {
    fn get_state_reader_from_latest_block(&self) -> Box<dyn MempoolStateReader>;
    fn get_state_reader(&self, block_number: BlockNumber) -> Box<dyn MempoolStateReader>;
}

// By default, a Box<dyn Trait> does not implement the trait of the object it contains.
// Therefore, for using the Box<dyn MempoolStateReader>, that the StateReaderFactory creates,
// we need to implement the MempoolStateReader trait for Box<dyn MempoolStateReader>.
impl MempoolStateReader for Box<dyn MempoolStateReader> {
    fn get_block_info(&self) -> Result<BlockInfo, StateError> {
        self.as_ref().get_block_info()
    }
}

impl BlockifierStateReader for Box<dyn MempoolStateReader> {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        self.as_ref().get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.as_ref().get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.as_ref().get_class_hash_at(contract_address)
    }

    fn get_compiled_contract_class(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
        self.as_ref().get_compiled_contract_class(class_hash)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.as_ref().get_compiled_class_hash(class_hash)
    }
}
