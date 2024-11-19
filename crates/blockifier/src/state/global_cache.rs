use std::sync::{Arc, Mutex, MutexGuard};

use cached::{Cached, SizedCache};
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;
use starknet_api::core::ClassHash;
#[cfg(feature = "cairo_native")]
use starknet_api::state::ContractClass as SierraContractClass;

use crate::execution::contract_class::RunnableContractClass;

// Note: `ContractClassLRUCache` key-value types must align with `ContractClassMapping`.
type ContractClassLRUCache<T> = SizedCache<ClassHash, T>;
pub type LockedContractClassCache<'a, T> = MutexGuard<'a, ContractClassLRUCache<T>>;
#[derive(Debug, Clone)]
// Thread-safe LRU cache for contract classes, optimized for inter-language sharing when
// `blockifier` compiles as a shared library.
// TODO(Yoni, 1/1/2025): consider defining CachedStateReader.
pub struct GlobalContractCache<T: Clone>(pub Arc<Mutex<ContractClassLRUCache<T>>>);

pub const GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST: usize = 100;

impl<T: Clone> GlobalContractCache<T> {
    /// Locks the cache for atomic access. Although conceptually shared, writing to this cache is
    /// only possible for one writer at a time.
    pub fn lock(&self) -> LockedContractClassCache<'_, T> {
        self.0.lock().expect("Global contract cache is poisoned.")
    }

    pub fn get(&self, class_hash: &ClassHash) -> Option<T> {
        self.lock().cache_get(class_hash).cloned()
    }

    pub fn set(&self, class_hash: ClassHash, contract_class: T) {
        self.lock().cache_set(class_hash, contract_class);
    }

    pub fn clear(&mut self) {
        self.lock().cache_clear();
    }

    pub fn new(cache_size: usize) -> Self {
        Self(Arc::new(Mutex::new(ContractClassLRUCache::<T>::with_size(cache_size))))
    }
}

#[cfg(feature = "cairo_native")]
pub struct GlobalContractCacheManager {
    pub casm_contract_class_cache: GlobalContractCache<RunnableContractClass>,
    pub native_contract_executor_cache: GlobalContractCache<Option<AotContractExecutor>>,
    pub sierra_contract_class_cache: GlobalContractCache<Arc<SierraContractClass>>,
}

#[cfg(feature = "cairo_native")]
impl GlobalContractCacheManager {
    pub fn get_casm_contract_class(&self, class_hash: &ClassHash) -> Option<RunnableContractClass> {
        self.casm_contract_class_cache.get(class_hash)
    }

    pub fn set_casm_contract_class(
        &self,
        class_hash: ClassHash,
        contract_class: RunnableContractClass,
    ) {
        self.casm_contract_class_cache.set(class_hash, contract_class);
    }

    pub fn get_casm_contract_executor(
        &self,
        class_hash: &ClassHash,
    ) -> Option<Option<AotContractExecutor>> {
        self.native_contract_executor_cache.get(class_hash)
    }

    pub fn set_native_contract_executor(
        &self,
        class_hash: ClassHash,
        contract_executor: Option<AotContractExecutor>,
    ) {
        self.native_contract_executor_cache.set(class_hash, contract_executor);
    }

    pub fn get_native_contract_executor(
        &self,
        class_hash: &ClassHash,
    ) -> Option<Option<AotContractExecutor>> {
        self.native_contract_executor_cache.get(class_hash)
    }

    pub fn set_sierra_contract_class(
        &self,
        class_hash: ClassHash,
        contract_class: Arc<SierraContractClass>,
    ) {
        self.sierra_contract_class_cache.set(class_hash, contract_class);
    }

    pub fn get_sierra_contract_class(
        &self,
        class_hash: &ClassHash,
    ) -> Option<Arc<SierraContractClass>> {
        self.sierra_contract_class_cache.get(class_hash)
    }

    pub fn new(cache_size: usize) -> Self {
        Self {
            casm_contract_class_cache: GlobalContractCache::new(cache_size),
            native_contract_executor_cache: GlobalContractCache::new(cache_size),
            sierra_contract_class_cache: GlobalContractCache::new(cache_size),
        }
    }

    pub fn clear(&mut self) {
        self.casm_contract_class_cache.clear();
        self.native_contract_executor_cache.clear();
        self.sierra_contract_class_cache.clear();
    }
}
