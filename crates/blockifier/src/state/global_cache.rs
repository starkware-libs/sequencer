use std::sync::{Arc, Mutex, MutexGuard};

use cached::{Cached, SizedCache};
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;
use starknet_api::core::ClassHash;
#[cfg(feature = "cairo_native")]
use starknet_api::state::ContractClass as SierraContractClass;

#[cfg(feature = "cairo_native")]
use crate::execution::contract_class::RunnableContractClass;

// Note: `ContractClassLRUCache` key-value types must align with `ContractClassMapping`.
type ContractClassLRUCache<T> = SizedCache<ClassHash, T>;
pub type LockedContractClassCache<'a, T> = MutexGuard<'a, ContractClassLRUCache<T>>;
#[derive(Debug, Clone)]
// Thread-safe LRU cache for contract classes, optimized for inter-language sharing when
// `blockifier` compiles as a shared library.
// TODO(Yoni, 1/1/2025): consider defining CachedStateReader.
pub struct GlobalContractCache<T: Clone>(pub Arc<Mutex<ContractClassLRUCache<T>>>);

#[cfg(feature = "cairo_native")]
#[derive(Debug, Clone)]
pub enum CachedCairoNative {
    Compiled(AotContractExecutor),
    CompilationFailed,
}

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
    pub casm_cache: GlobalContractCache<RunnableContractClass>,
    pub native_cache: GlobalContractCache<CachedCairoNative>,
    pub sierra_cache: GlobalContractCache<Arc<SierraContractClass>>,
}

#[cfg(feature = "cairo_native")]
impl GlobalContractCacheManager {
    pub fn get_casm(&self, class_hash: &ClassHash) -> Option<RunnableContractClass> {
        self.casm_cache.get(class_hash)
    }

    pub fn set_casm(&self, class_hash: ClassHash, contract_class: RunnableContractClass) {
        self.casm_cache.set(class_hash, contract_class);
    }

    pub fn get_native(&self, class_hash: &ClassHash) -> Option<CachedCairoNative> {
        self.native_cache.get(class_hash)
    }

    pub fn set_native(&self, class_hash: ClassHash, contract_executor: CachedCairoNative) {
        self.native_cache.set(class_hash, contract_executor);
    }

    pub fn get_sierra(&self, class_hash: &ClassHash) -> Option<Arc<SierraContractClass>> {
        self.sierra_cache.get(class_hash)
    }

    pub fn set_sierra(&self, class_hash: ClassHash, contract_class: Arc<SierraContractClass>) {
        self.sierra_cache.set(class_hash, contract_class);
    }

    pub fn new(cache_size: usize) -> Self {
        Self {
            casm_cache: GlobalContractCache::new(cache_size),
            native_cache: GlobalContractCache::new(cache_size),
            sierra_cache: GlobalContractCache::new(cache_size),
        }
    }

    pub fn clear(&mut self) {
        self.casm_cache.clear();
        self.native_cache.clear();
        self.sierra_cache.clear();
    }
}
