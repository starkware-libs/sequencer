use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::RunnableCompiledClass;
use crate::metrics::{CLASS_CACHE_HITS, CLASS_CACHE_MISSES};
use crate::state::contract_class_manager::ContractClassManager;
use crate::state::errors::StateError;
use crate::state::global_cache::CompiledClasses;
use crate::state::state_api::{StateReader, StateResult};
#[cfg(test)]
#[path = "state_reader_and_contract_manager_test.rs"]
pub mod state_reader_and_contract_manager_test;

pub trait FetchCompiledClasses: StateReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses>;

    /// Returns whether the given Cairo1 class is declared.
    fn is_declared(&self, class_hash: ClassHash) -> StateResult<bool>;
}

pub struct StateReaderAndContractManager<S: FetchCompiledClasses> {
    pub state_reader: S,
    pub contract_class_manager: ContractClassManager,
}

impl<S: FetchCompiledClasses> StateReaderAndContractManager<S> {
    fn get_compiled_from_class_manager(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<RunnableCompiledClass> {
        if let Some(runnable_class) = self.contract_class_manager.get_runnable(&class_hash) {
            match &runnable_class {
                RunnableCompiledClass::V0(_) => {}
                _ => {
                    // The Cairo1 class is cached; verify it is declared,
                    // since existence in the cache does not guarantee that
                    // (it might contain a declared class from a reverted block, for example).
                    if !self.state_reader.is_declared(class_hash)? {
                        return Err(StateError::UndeclaredClassHash(class_hash));
                    }
                }
            }
            CLASS_CACHE_HITS.increment(1);
            self.update_native_metrics(&runnable_class);
            return Ok(runnable_class);
        }
        CLASS_CACHE_MISSES.increment(1);

        let compiled_class = self.state_reader.get_compiled_classes(class_hash)?;
        self.contract_class_manager.set_and_compile(class_hash, compiled_class.clone());
        // Access the cache again in case the class was compiled.
        let runnable_class =
            self.contract_class_manager.get_runnable(&class_hash).unwrap_or_else(|| {
                // Edge case that should not be happen if the cache size is big enough.
                // TODO(Yoni): consider having an atomic set-and-get.
                log::error!("Class is missing immediately after being cached.");
                compiled_class.to_runnable()
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
}

impl<S: FetchCompiledClasses> StateReader for StateReaderAndContractManager<S> {
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

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.state_reader.get_compiled_class_hash(class_hash)
    }

    /// Returns the compiled class hash v2 for the given class hash.
    /// The function assumes that the class hash is already declared,
    /// and of cairo1 contract class.
    fn get_compiled_class_hash_v2(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        // First, try getting from class manager cache.
        match self.contract_class_manager.get_compiled_class_hash_v2(&class_hash) {
            Some(compiled_class_hash) => Ok(compiled_class_hash),
            None => {
                // Not in cache → fetch from state reader.
                let compiled_class_hash =
                    self.state_reader.get_compiled_class_hash_v2(class_hash)?;
                // Verify that the returned compiled_class_hash_v2 is not the default value.
                // default value is used to mark classes that are not declared or cairo0 classes.
                assert_ne!(
                    compiled_class_hash,
                    CompiledClassHash::default(),
                    "Default compiled class hash is for marking classes that are not declared or \
                     cairo0 classes. class_hash: {class_hash:?}"
                );
                // Store in cache.
                self.contract_class_manager
                    .set_compiled_class_hash_v2(class_hash, compiled_class_hash);

                Ok(compiled_class_hash)
            }
        }
    }
}
