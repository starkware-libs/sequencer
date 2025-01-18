use std::sync::Arc;

use starknet_api::class_cache::GlobalContractCache;
use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;

use crate::execution::contract_class::RunnableCompiledClass;
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;

pub const GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST: usize = 400;

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
