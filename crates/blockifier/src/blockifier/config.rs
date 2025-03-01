use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::core::ClassHash;
use starknet_sierra_multicompile::config::SierraCompilationConfig;

use crate::blockifier::transaction_executor::DEFAULT_STACK_SIZE;
use crate::state::contract_class_manager::DEFAULT_COMPILATION_REQUEST_CHANNEL_SIZE;
use crate::state::global_cache::GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST;

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
}

impl Default for TransactionExecutorConfig {
    fn default() -> Self {
        Self { concurrency_config: ConcurrencyConfig::default(), stack_size: DEFAULT_STACK_SIZE }
    }
}

impl SerializeConfig for TransactionExecutorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = append_sub_config_name(self.concurrency_config.dump(), "concurrency_config");
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
        Self { cairo_native_run_config, ..Default::default() }
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
        dump.append(&mut append_sub_config_name(
            self.cairo_native_run_config.dump(),
            "cairo_native_run_config",
        ));
        dump.append(&mut append_sub_config_name(
            self.native_compiler_config.dump(),
            "native_compiler_config",
        ));
        dump
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CairoNativeRunConfig {
    pub run_cairo_native: bool,
    pub wait_on_native_compilation: bool,
    pub channel_size: usize,
    #[serde(deserialize_with = "deserialize_optional_vec_class_hash")]
    pub contract_to_compile_natively: Option<Vec<ClassHash>>, /* if 'None' compile all contracts
                                                               * natively. */
}

impl Default for CairoNativeRunConfig {
    fn default() -> Self {
        Self {
            run_cairo_native: false,
            wait_on_native_compilation: false,
            channel_size: DEFAULT_COMPILATION_REQUEST_CHANNEL_SIZE,
            contract_to_compile_natively: None,
        }
    }
}

impl SerializeConfig for CairoNativeRunConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
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
        ]);
        dump.extend([ser_param(
            "contract_to_compile_natively",
            &serialize_optional_vec_class_hash(&self.contract_to_compile_natively),
            "Contracts to compile natively (None means all).",
            ParamPrivacyInput::Public,
        )]);
        dump
    }
}

// TODO(AvivG): Move to a more accurate location.
fn serialize_optional_vec_class_hash(optional_vector: &Option<Vec<ClassHash>>) -> String {
    match optional_vector {
        // TODO (AvivG): change "" to 'compile all contracts with native' or other more
        // descriptive name.
        None => "".to_owned(),
        Some(vector) => {
            format!(
                "0x{}",
                vector
                    .iter()
                    .map(|class_hash| format!("{:#x}", class_hash.0))
                    .collect::<Vec<String>>()
                    .join("")
            )
        }
    }
}

fn deserialize_optional_vec_class_hash<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<ClassHash>>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;

    match s.as_deref() {
        None | Some("") => Ok(None),
        Some(hex_string) => {
            let classes: Result<Vec<ClassHash>, _> = hex_string
                .split("0x") // Split on "0x"
                .filter(|s| !s.is_empty()) // Ignore empty parts
                .map(|class_hash| {
                    u64::from_str_radix(class_hash, 16)
                        .map(|class_hash| ClassHash(class_hash.into())) // Convert to ClassHash
                        .map_err(|e| serde::de::Error::custom(format!(
                            "Couldn't deserialize vector. Failed to parse class_hash: {} {}",
                            class_hash, e
                        )))
                })
                .collect();
            classes.map(Some)
        }
    }
}
