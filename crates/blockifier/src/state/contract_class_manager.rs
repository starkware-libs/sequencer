pub const DEFAULT_COMPILATION_REQUEST_CHANNEL_SIZE: usize = 2000;

#[cfg(feature = "cairo_native")]
pub type ContractClassManager = crate::state::native_class_manager::NativeClassManager;

#[cfg(not(feature = "cairo_native"))]
pub mod trivial_class_manager {
    #[cfg(any(feature = "testing", test))]
    use cached::Cached;
    use starknet_api::core::ClassHash;

    use crate::blockifier::config::ContractClassManagerConfig;
    use crate::execution::contract_class::RunnableCompiledClass;
    use crate::state::global_cache::{CachedClass, RawClassCache};

    pub type ContractClassManager = RawClassCache;

    // Trivial implementation of the class manager for Native-less projects.
    impl ContractClassManager {
        pub fn start(config: ContractClassManagerConfig) -> Self {
            assert!(
                !config.cairo_native_run_config.run_cairo_native,
                "Cairo Native feature is off."
            );
            Self::new(config.contract_cache_size)
        }

        pub fn get_runnable(&self, class_hash: &ClassHash) -> Option<RunnableCompiledClass> {
            Some(self.get(class_hash)?.to_runnable())
        }

        pub fn set_and_compile(&self, class_hash: ClassHash, compiled_class: CachedClass) {
            self.set(class_hash, compiled_class);
        }

        #[cfg(any(feature = "testing", test))]
        pub fn get_cache_size(&self) -> usize {
            self.lock().cache_size()
        }
    }
}

#[cfg(not(feature = "cairo_native"))]
pub use trivial_class_manager::ContractClassManager;
