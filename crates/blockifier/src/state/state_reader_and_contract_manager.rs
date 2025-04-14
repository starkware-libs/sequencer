use std::sync::Arc;

use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::{CompiledClassV0, CompiledClassV1, RunnableCompiledClass};
use crate::metrics::{CLASS_CACHE_HITS, CLASS_CACHE_MISSES};
use crate::state::contract_class_manager::ContractClassManager;
use crate::state::global_cache::CachedClass;
use crate::state::state_api::{StateReader, StateResult};

#[derive(Debug, Clone)]
pub enum CompiledClass {
    V0(CompiledClassV0),
    V1(CompiledClassV1, Arc<SierraContractClass>),
}

pub struct StateReaderAndContractManger<S: StateReader> {
    pub state_reader: S,
    pub contract_class_manager: ContractClassManager,
}

impl From<CompiledClass> for CachedClass {
    fn from(compiled_class: CompiledClass) -> Self {
        match compiled_class {
            CompiledClass::V0(class) => CachedClass::V0(class),
            CompiledClass::V1(class, sierra_class) => CachedClass::V1(class, sierra_class),
        }
    }
}

impl<S: StateReader> StateReaderAndContractManger<S> {
    fn get_compiled_from_class_manager(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<RunnableCompiledClass> {
        // Assumption: the global cache is cleared upon reverted blocks.
        if let Some(runnable_class) = self.contract_class_manager.get_runnable(&class_hash) {
            CLASS_CACHE_HITS.increment(1);
            self.update_native_metrics(&runnable_class);
            return Ok(runnable_class);
        }
        CLASS_CACHE_MISSES.increment(1);

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
        self.update_native_metrics(&runnable_class);
        Ok(runnable_class)
    }

    fn update_native_metrics(&self, _runnable_class: &RunnableCompiledClass) {
        #[cfg(feature = "cairo_native")]
        {
            if matches!(_runnable_class, RunnableCompiledClass::V1Native(_)) {
                crate::metrics::NATIVE_CLASS_RETURNED.increment(1);
            }
        }
    }

    fn get_cached_class(&self, class_hash: ClassHash) -> StateResult<CachedClass> {
        self.state_reader.get_sierra_and_casm(class_hash).map(|class| class.into())
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
