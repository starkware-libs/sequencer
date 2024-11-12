use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};

use crate::state::global_cache::GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TransactionExecutorConfig {
    pub concurrency_config: ConcurrencyConfig,
}
impl TransactionExecutorConfig {
    #[cfg(any(test, feature = "testing"))]
    pub fn create_for_testing(concurrency_enabled: bool) -> Self {
        Self { concurrency_config: ConcurrencyConfig::create_for_testing(concurrency_enabled) }
    }
}

impl SerializeConfig for TransactionExecutorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        append_sub_config_name(self.concurrency_config.dump(), "concurrency_config")
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
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

impl SerializeConfig for ConcurrencyConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "enabled",
                &self.enabled,
                "Enables concurrency of transaction execution.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "n_workers",
                &self.n_workers,
                "Number of parallel transaction execution workers.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chunk_size",
                &self.chunk_size,
                "The size of the transaction chunk executed in parallel.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CairoNativeConfig {
    pub run_cairo_native: bool,
    pub block_compilation: bool,
    pub global_contract_cache_size: usize,
}
impl CairoNativeConfig {
    pub fn default() -> Self {
        Self {
            run_cairo_native: false,
            block_compilation: false,
            global_contract_cache_size: GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST,
        }
    }
}

impl SerializeConfig for CairoNativeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "run_cairo_native",
                &self.run_cairo_native,
                "Enables Cairo native execution.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "block_compilation",
                &self.block_compilation,
                "Block Sequencer main program while compiling sierra, for testing.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "global_contract_cache_size",
                &self.global_contract_cache_size,
                "The size of the global contract cache.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
