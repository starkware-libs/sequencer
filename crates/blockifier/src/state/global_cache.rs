use std::sync::{Arc, Mutex, MutexGuard};

use cached::{Cached, SizedCache};
use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;

use crate::execution::contract_class::RunnableCompiledClass;
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;

type ContractLRUCache<T> = SizedCache<ClassHash, T>;
pub type LockedClassCache<'a, T> = MutexGuard<'a, ContractLRUCache<T>>;
#[derive(Debug, Clone)]
// Thread-safe LRU cache for contract classes (Seirra or compiled Casm/Native), optimized for
// inter-language sharing when `blockifier` compiles as a shared library.
// TODO(Yoni, 1/1/2025): consider defining CachedStateReader.
pub struct GlobalContractCache<T: Clone>(pub Arc<Mutex<ContractLRUCache<T>>>);

#[derive(Debug, Clone)]
pub enum CachedCasm {
    WithoutSierra(RunnableCompiledClass),
    WithSierra(RunnableCompiledClass, Arc<SierraContractClass>),
}
impl CachedCasm {
    pub fn to_runnable_casm(&self) -> RunnableCompiledClass {
        match self {
            CachedCasm::WithoutSierra(casm) | CachedCasm::WithSierra(casm, _) => casm.clone(),
        }
    }
}

#[cfg(feature = "cairo_native")]
#[derive(Debug, Clone)]
pub enum CachedCairoNative {
    Compiled(NativeCompiledClassV1),
    CompilationFailed,
}

pub const GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST: usize = 400;

impl<T: Clone> GlobalContractCache<T> {
    /// Locks the cache for atomic access. Although conceptually shared, writing to this cache is
    /// only possible for one writer at a time.
    pub fn lock(&self) -> LockedClassCache<'_, T> {
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
        Self(Arc::new(Mutex::new(ContractLRUCache::<T>::with_size(cache_size))))
    }
}

#[derive(Clone)]
pub struct ContractCaches {
    pub casm_cache: GlobalContractCache<CachedCasm>,
    #[cfg(feature = "cairo_native")]
    pub native_cache: GlobalContractCache<CachedCairoNative>,
}

impl ContractCaches {
    pub fn get_casm(&self, class_hash: &ClassHash) -> Option<CachedCasm> {
        self.casm_cache.get(class_hash)
    }

    pub fn set_casm(&self, class_hash: ClassHash, compiled_class: CachedCasm) {
        self.casm_cache.set(class_hash, compiled_class);
    }

    #[cfg(feature = "cairo_native")]
    pub fn get_native(&self, class_hash: &ClassHash) -> Option<CachedCairoNative> {
        self.native_cache.get(class_hash)
    }

    #[cfg(feature = "cairo_native")]
    pub fn set_native(&self, class_hash: ClassHash, contract_executor: CachedCairoNative) {
        self.native_cache.set(class_hash, contract_executor);
    }

    pub fn new(cache_size: usize) -> Self {
        Self {
            casm_cache: GlobalContractCache::new(cache_size),
            #[cfg(feature = "cairo_native")]
            native_cache: GlobalContractCache::new(cache_size),
        }
    }

    pub fn clear(&mut self) {
        self.casm_cache.clear();
        #[cfg(feature = "cairo_native")]
        self.native_cache.clear();
    }
}
