use std::sync::Arc;

use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::{CompiledClassV0, CompiledClassV1, RunnableCompiledClass};
use crate::state::contract_class_manager::ContractClassManager;
use crate::state::global_cache::CachedClass;
use crate::state::state_api::{StateReader, StateResult};


pub struct StateReaderAndContractManger<S: StateReader> {
    pub state_reader: S,
    pub contract_class_manager: ContractClassManager,
}

impl<S: StateReader> StateReaderAndContractManger<S> {
    fn get_compiled_from_class_manager(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<RunnableCompiledClass> {
        // TODO(AvivG): update metrics.
        // Assumption: the global cache is cleared upon reverted blocks.
        if let Some(runnable_class) = self.contract_class_manager.get_runnable(&class_hash) {
            return Ok(runnable_class);
        }

        let cached_class = self.get_cached_class(class_hash)?;
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

    fn get_cached_class(&self, _class_hash: ClassHash) -> StateResult<CachedClass> {
        // TODO(AvivG): Implement this function once exists:
        // StateReader::get_sierra_class(class_hash: ClassHash) -> StateResult<SierraContractClass>
        todo!();
    }
}

impl<S: StateReader> StateReader for StateReaderAndContractManger<S> {
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
        self.get_compiled_from_class_manager(class_hash)
    }

    fn get_compiled_class_hash(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<starknet_api::core::CompiledClassHash> {
        self.state_reader.get_compiled_class_hash(class_hash)
    }
}
