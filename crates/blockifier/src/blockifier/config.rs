use std::collections::BTreeMap;

use apollo_compile_to_native_types::SierraCompilationConfig;
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use starknet_api::core::ClassHash;
use starknet_types_core::felt::Felt;

use crate::blockifier::transaction_executor::DEFAULT_STACK_SIZE;
use crate::state::contract_class_manager::DEFAULT_COMPILATION_REQUEST_CHANNEL_SIZE;
use crate::state::global_cache::GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST;

#[cfg(test)]
#[path = "config_test.rs"]
pub mod config_test;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransactionExecutorConfig {
    pub concurrency_config: ConcurrencyConfig,
    pub stack_size: usize,
}
impl TransactionExecutorConfig {
    #[cfg(any(test, feature = "testing", feature = "native_blockifier"))]
    pub fn create_for_testing(concurrency_enabled: bool) -> Self {
        Self {
            concurrency_config: ConcurrencyConfig::create_for_testing(concurrency_enabled),
            stack_size: DEFAULT_STACK_SIZE,
        }
    }

    pub fn get_worker_pool_config(&self) -> WorkerPoolConfig {
        WorkerPoolConfig {
            n_workers: self.concurrency_config.n_workers,
            stack_size: self.stack_size,
        }
    }
}

impl Default for TransactionExecutorConfig {
    fn default() -> Self {
        Self { concurrency_config: ConcurrencyConfig::default(), stack_size: DEFAULT_STACK_SIZE }
    }
}

impl SerializeConfig for TransactionExecutorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump =
            prepend_sub_config_name(self.concurrency_config.dump(), "concurrency_config");
        dump.append(&mut BTreeMap::from([ser_param(
            "stack_size",
            &self.stack_size,
            "The thread stack size (proportional to the maximal gas of a transaction).",
            ParamPrivacyInput::Public,
        )]));
        dump
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
pub struct WorkerPoolConfig {
    pub n_workers: usize,
    pub stack_size: usize,
}
impl WorkerPoolConfig {
    #[cfg(any(test, feature = "testing"))]
    pub fn create_for_testing() -> Self {
        Self { n_workers: 4, stack_size: DEFAULT_STACK_SIZE }
    }
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self { n_workers: 1, stack_size: DEFAULT_STACK_SIZE }
    }
}

impl SerializeConfig for WorkerPoolConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "n_workers",
                &self.n_workers,
                "Number of parallel transaction execution workers.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "stack_size",
                &self.stack_size,
                "The thread stack size (proportional to the maximal gas of a transaction).",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContractClassManagerConfig {
    pub cairo_native_run_config: CairoNativeRunConfig,
    pub contract_cache_size: usize,
    pub native_compiler_config: SierraCompilationConfig,
}

impl Default for ContractClassManagerConfig {
    fn default() -> Self {
        Self {
            cairo_native_run_config: CairoNativeRunConfig::default(),
            contract_cache_size: GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST,
            native_compiler_config: SierraCompilationConfig::default(),
        }
    }
}

impl ContractClassManagerConfig {
    #[cfg(any(test, feature = "testing", feature = "native_blockifier"))]
    pub fn create_for_testing(run_cairo_native: bool, wait_on_native_compilation: bool) -> Self {
        let cairo_native_run_config = CairoNativeRunConfig {
            run_cairo_native,
            wait_on_native_compilation,
            ..Default::default()
        };
        let native_compiler_config = SierraCompilationConfig::create_for_testing();
        Self { cairo_native_run_config, native_compiler_config, ..Default::default() }
    }
}

impl SerializeConfig for ContractClassManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([ser_param(
            "contract_cache_size",
            &self.contract_cache_size,
            "The size of the global contract cache.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut prepend_sub_config_name(
            self.cairo_native_run_config.dump(),
            "cairo_native_run_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.native_compiler_config.dump(),
            "native_compiler_config",
        ));
        dump
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativeClassesWhitelist {
    All,
    Limited(Vec<ClassHash>),
}

impl<'de> Deserialize<'de> for NativeClassesWhitelist {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: String = <String as serde::Deserialize>::deserialize(deserializer)?;

        if raw == "All" {
            return Ok(NativeClassesWhitelist::All);
        }

        // Accept a stringified list of hex values, e.g. "[0x1234, 0x5678]".
        // Also tolerate quotes around individual items: "[\"0x1234\", 0x5678]".
        let trimmed = raw.trim();
        if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
            return Err(de::Error::custom(format!(
                "invalid native_classes_whitelist string: expected \"All\" or stringified list \
                 like \"[0x.., 0x..]\", got: {}",
                raw
            )));
        }

        let inner = &trimmed[1..trimmed.len() - 1];
        let mut hashes: Vec<ClassHash> = Vec::new();
        let inner = inner.trim();
        if inner.is_empty() {
            return Ok(NativeClassesWhitelist::Limited(hashes));
        }

        for part in inner.split(',') {
            let token = part.trim();
            if token.is_empty() {
                return Err(de::Error::custom(
                    "invalid native_classes_whitelist string: empty element in list",
                ));
            }
            // Expect hex values with 0x/0X prefix.
            if !(token.starts_with("0x") || token.starts_with("0X")) {
                return Err(de::Error::custom(format!(
                    "invalid class hash (missing 0x prefix): {}",
                    token
                )));
            }
            let felt = Felt::from_hex(token)
                .map_err(|_| de::Error::custom(format!("invalid hex in class hash: {}", token)))?;
            hashes.push(ClassHash(felt));
        }

        Ok(NativeClassesWhitelist::Limited(hashes))
    }
}

impl Serialize for NativeClassesWhitelist {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            NativeClassesWhitelist::All => serializer.serialize_str("All"),
            NativeClassesWhitelist::Limited(vec) => {
                // Serialize as stringified list of bare hexes: "[0x..., 0x...]"
                let joined = vec.iter().map(|h| h.0.to_hex_string()).collect::<Vec<_>>().join(", ");
                let s = format!("[{}]", joined);
                serializer.serialize_str(&s)
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CairoNativeRunConfig {
    pub run_cairo_native: bool,
    pub wait_on_native_compilation: bool,
    pub channel_size: usize,
    pub native_classes_whitelist: NativeClassesWhitelist,
    pub panic_on_compilation_failure: bool,
}

impl Default for CairoNativeRunConfig {
    fn default() -> Self {
        Self {
            #[cfg(feature = "cairo_native")]
            run_cairo_native: true,
            #[cfg(not(feature = "cairo_native"))]
            run_cairo_native: false,
            wait_on_native_compilation: false,
            channel_size: DEFAULT_COMPILATION_REQUEST_CHANNEL_SIZE,
            native_classes_whitelist: NativeClassesWhitelist::All,
            panic_on_compilation_failure: false,
        }
    }
}

impl SerializeConfig for CairoNativeRunConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "run_cairo_native",
                &self.run_cairo_native,
                "Enables Cairo native execution.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "wait_on_native_compilation",
                &self.wait_on_native_compilation,
                "Block Sequencer main program while compiling sierra, for testing.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "channel_size",
                &self.channel_size,
                "The size of the compilation request channel.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "native_classes_whitelist",
                &self.native_classes_whitelist,
                "Specifies whether to execute all class hashes or only specific ones using Cairo \
                 native. If limited, a specific list of class hashes is provided.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "panic_on_compilation_failure",
                &self.panic_on_compilation_failure,
                "Whether to panic on compilation failure.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
