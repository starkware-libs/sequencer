use apollo_state_sync_types::communication::StateSyncClientResult;
use async_trait::async_trait;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::FetchCompiledClasses;
#[cfg(test)]
use mockall::automock;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::fixed_block_state_reader::GatewayFixedBlockStateReader;
#[cfg_attr(test, automock)]
#[async_trait]
pub trait StateReaderFactory: Send + Sync {
    async fn get_blockifier_state_reader_and_gateway_fixed_block_from_latest_block(
        &self,
    ) -> StateSyncClientResult<(
        Box<dyn GatewayStateReaderWithCompiledClasses>,
        Box<dyn GatewayFixedBlockStateReader>,
    )>;
}

// TODO(Arni): Delete this trait, once we replace `dyn GatewayStateReaderWithCompiledClasses` with
// generics.
pub trait GatewayStateReaderWithCompiledClasses: FetchCompiledClasses + Send + Sync {}

impl BlockifierStateReader for Box<dyn GatewayStateReaderWithCompiledClasses> {
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

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.as_ref().get_compiled_class(class_hash)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.as_ref().get_compiled_class_hash(class_hash)
    }
}

impl FetchCompiledClasses for Box<dyn GatewayStateReaderWithCompiledClasses> {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        self.as_ref().get_compiled_classes(class_hash)
    }

    fn is_declared(&self, class_hash: ClassHash) -> StateResult<bool> {
        self.as_ref().is_declared(class_hash)
    }
}
