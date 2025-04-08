use starknet_api::core::ClassHash;

use super::global_cache::CachedClass;
use super::state_api::{StateReader, StateResult};
use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::contract_class_manager::ContractClassManager;

pub trait SupportsClassCaching {
    fn get_cached_class(&self, class_hash: ClassHash) -> StateResult<CachedClass>;
}

pub trait StateReaderSupportingCompilation: StateReader + SupportsClassCaching {}

impl<T: StateReader + SupportsClassCaching> StateReaderSupportingCompilation for T {}

pub struct StateReaderWithClassCompilation {
    pub state_reader: Box<dyn StateReaderSupportingCompilation>,
    pub contract_class_manager: ContractClassManager,
}

impl StateReaderWithClassCompilation {
    fn _get_runnable_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
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
