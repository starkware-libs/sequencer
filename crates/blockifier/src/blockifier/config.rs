use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};

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
                "Whether to use concurrency.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "n_workers",
                &self.n_workers,
                "Number of workers.",
                ParamPrivacyInput::Public,
            ),
            ser_param("chunk_size", &self.chunk_size, "Chunk size.", ParamPrivacyInput::Public),
        ])
    }
}
