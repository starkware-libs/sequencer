use std::sync::Arc;

use starknet_api::class_cache::GlobalContractCache;
use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;

use crate::execution::contract_class::{CompiledClassV0, CompiledClassV1, RunnableCompiledClass};
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;

pub const GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST: usize = 600;

#[derive(Debug, Clone)]
pub enum CachedClass {
    V0(CompiledClassV0),
    V1(CompiledClassV1, Arc<SierraContractClass>),
    #[cfg(feature = "cairo_native")]
    V1Native(CachedCairoNative),
}
impl CachedClass {
    pub fn to_runnable(&self) -> RunnableCompiledClass {
        match self {
            CachedClass::V0(compiled_class_v0) => {
                RunnableCompiledClass::V0(compiled_class_v0.clone())
            }
            CachedClass::V1(compiled_class_v1, _sierra_contract_class) => {
                RunnableCompiledClass::V1(compiled_class_v1.clone())
            }
            #[cfg(feature = "cairo_native")]
            CachedClass::V1Native(cached_cairo_native) => match cached_cairo_native {
                CachedCairoNative::Compiled(native_compiled_class_v1) => {
                    RunnableCompiledClass::V1Native(native_compiled_class_v1.clone())
                }
                CachedCairoNative::CompilationFailed(compiled_class_v1) => {
                    RunnableCompiledClass::V1(compiled_class_v1.clone())
                }
            },
        }
    }
}

pub type RawClassCache = GlobalContractCache<CachedClass>;

#[cfg(feature = "cairo_native")]
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub enum CachedCairoNative {
    Compiled(NativeCompiledClassV1),
    CompilationFailed(CompiledClassV1),
}

<<<<<<< HEAD
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
||||||| 91889fd5e
pub const GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST: usize = 600;

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
=======
pub const GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST: usize = 600;

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
>>>>>>> origin/main-v0.13.4
