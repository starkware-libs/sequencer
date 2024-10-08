#[derive(Debug, Default, Clone)]
pub struct TransactionExecutorConfig {
    pub concurrency_config: ConcurrencyConfig,
}
impl TransactionExecutorConfig {
    #[cfg(any(test, feature = "testing"))]
    pub fn create_for_testing(concurrency_enabled: bool) -> Self {
        Self { concurrency_config: ConcurrencyConfig::create_for_testing(concurrency_enabled) }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ConcurrencyConfig {
    pub enabled: bool,
    pub n_workers: usize,
    pub chunk_size: usize,
}

impl ConcurrencyConfig {
    pub fn create_for_testing(concurrency_enabled: bool) -> Self {
        if concurrency_enabled {
            return Self { enabled: true, n_workers: 4, chunk_size: 64 };
        }
        Self { enabled: false, n_workers: 0, chunk_size: 0 }
    }
}
