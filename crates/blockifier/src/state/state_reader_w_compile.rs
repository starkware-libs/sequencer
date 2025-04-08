use starknet_api::core::ClassHash;
use starknet_types_core::felt::Felt;

use super::global_cache::CachedClass;
use super::state_api::{StateReader, StateResult};
use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::contract_class_manager::ContractClassManager;

pub trait CompilableStateReader {
    fn get_cached_class(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<CachedClass>;
}

pub trait StateReaderWithCompileTrait: StateReader + CompilableStateReader {}

impl<T: StateReader + CompilableStateReader> StateReaderWithCompileTrait for T {}

pub struct StateReaderWithCompile {
    pub state_reader: Box<dyn StateReaderWithCompileTrait>,
    pub contract_class_manager: ContractClassManager,
}

impl StateReaderWithCompile {
    fn get_runnable_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        // Assumption: the global cache is cleared upon reverted blocks.
        if let Some(runnable_class) = self.contract_class_manager.get_runnable(&class_hash) {
            return Ok(runnable_class);
        }

        let cached_class = self.state_reader.get_cached_class(class_hash)?;
        self.contract_class_manager.set_and_compile(class_hash, cached_class.clone());
        // Access the cache again in case the class was compiled.
        let runnable_class =
            self.contract_class_manager.get_runnable(&class_hash).unwrap_or_else(|| {
                // Edge case that should not be happen if the cache size is big enough.
                // TODO(Yoni): consider having an atomic set-and-get.
                log::error!("Class is missing immediately after being cached.");
                cached_class.to_runnable()
            });
        Ok(runnable_class)
    }
}

impl StateReader for StateReaderWithCompile {
    fn get_storage_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
        key: starknet_api::state::StorageKey,
    ) -> StateResult<Felt> {
        self.state_reader.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
    ) -> StateResult<starknet_api::core::Nonce> {
        self.state_reader.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
    ) -> StateResult<ClassHash> {
        self.state_reader.get_class_hash_at(contract_address)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.get_runnable_class(class_hash)
    }

    fn get_compiled_class_hash(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<starknet_api::core::CompiledClassHash> {
        self.state_reader.get_compiled_class_hash(class_hash)
    }
}
