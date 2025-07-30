#![allow(non_local_definitions)]

use std::collections::HashMap;
use std::path::PathBuf;

use apollo_compile_to_native::config::SierraCompilationConfig;
use blockifier::abi::constants;
use blockifier::blockifier::config::{
    CairoNativeRunConfig,
    ConcurrencyConfig,
    ContractClassManagerConfig,
    NativeClassesWhitelist,
};
use blockifier::blockifier::transaction_executor::CompiledClassHashesForMigration;
use blockifier::blockifier_versioned_constants::VersionedConstantsOverrides;
use blockifier::bouncer::{BouncerConfig, BouncerWeights, BuiltinWeights, CasmHashComputationData};
use blockifier::state::contract_class_manager::DEFAULT_COMPILATION_REQUEST_CHANNEL_SIZE;
use blockifier::state::global_cache::GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use pyo3::prelude::*;
use starknet_api::core::ClassHash;
use starknet_api::execution_resources::{Builtin, GasAmount};

use crate::errors::{NativeBlockifierError, NativeBlockifierResult};
use crate::py_utils::PyFelt;

// From Rust to Python.

#[pyclass]
#[derive(Clone, Default)]
pub struct PyExecutionResources {
    #[pyo3(get)]
    pub n_steps: usize,
    #[pyo3(get)]
    pub builtin_instance_counter: HashMap<String, usize>,
    #[pyo3(get)]
    pub n_memory_holes: usize,
}

impl From<ExecutionResources> for PyExecutionResources {
    fn from(resources: ExecutionResources) -> Self {
        Self {
            n_steps: resources.n_steps,
            builtin_instance_counter: resources
                .builtin_instance_counter
                .iter()
                .map(|(builtin, count)| (builtin.to_str_with_suffix().to_string(), *count))
                .collect(),
            n_memory_holes: resources.n_memory_holes,
        }
    }
}

#[pyclass]
pub struct PyCasmHashComputationData {
    #[pyo3(get)]
    pub class_hash_to_casm_hash_computation_gas: HashMap<PyFelt, u64>,
    #[pyo3(get)]
    pub gas_without_casm_hash_computation: u64,
}

impl From<CasmHashComputationData> for PyCasmHashComputationData {
    fn from(data: CasmHashComputationData) -> Self {
        Self {
            class_hash_to_casm_hash_computation_gas: data
                .class_hash_to_casm_hash_computation_gas
                .iter()
                .map(|(class_hash, gas)| ((*class_hash).into(), gas.0))
                .collect(),
            gas_without_casm_hash_computation: data.gas_without_casm_hash_computation.0,
        }
    }
}

// From Python to Rust.
#[pyclass]
#[derive(Clone)]
pub struct PyVersionedConstantsOverrides {
    pub validate_max_n_steps: u32,
    pub max_recursion_depth: usize,
    pub invoke_tx_max_n_steps: u32,
    pub max_n_events: usize,
}

#[pymethods]
impl PyVersionedConstantsOverrides {
    #[new]
    #[pyo3(signature = (validate_max_n_steps, max_recursion_depth, invoke_tx_max_n_steps, max_n_events))]
    pub fn create(
        validate_max_n_steps: u32,
        max_recursion_depth: usize,
        invoke_tx_max_n_steps: u32,
        max_n_events: usize,
    ) -> Self {
        Self { validate_max_n_steps, max_recursion_depth, invoke_tx_max_n_steps, max_n_events }
    }
}

impl From<PyVersionedConstantsOverrides> for VersionedConstantsOverrides {
    fn from(py_versioned_constants_overrides: PyVersionedConstantsOverrides) -> Self {
        let PyVersionedConstantsOverrides {
            validate_max_n_steps,
            max_recursion_depth,
            invoke_tx_max_n_steps,
            max_n_events,
        } = py_versioned_constants_overrides;
        Self { validate_max_n_steps, max_recursion_depth, invoke_tx_max_n_steps, max_n_events }
    }
}

#[derive(Clone, Debug, FromPyObject)]
pub struct PyBouncerConfig {
    pub full_total_weights: HashMap<String, usize>,
    pub builtin_weights: HashMap<String, usize>,
    pub blake_weight: usize,
}

impl TryFrom<PyBouncerConfig> for BouncerConfig {
    type Error = NativeBlockifierError;
    fn try_from(py_bouncer_config: PyBouncerConfig) -> Result<Self, Self::Error> {
        Ok(BouncerConfig {
            block_max_capacity: hash_map_into_bouncer_weights(
                py_bouncer_config.full_total_weights.clone(),
            )?,
            builtin_weights: hash_map_into_builtin_weights(
                py_bouncer_config.builtin_weights.clone(),
            )?,
            blake_weight: py_bouncer_config.blake_weight,
        })
    }
}

fn hash_map_into_bouncer_weights(
    mut data: HashMap<String, usize>,
) -> NativeBlockifierResult<BouncerWeights> {
    let l1_gas = data.remove(constants::L1_GAS_USAGE).expect("gas_weight must be present");
    let message_segment_length = data
        .remove(constants::MESSAGE_SEGMENT_LENGTH)
        .expect("message_segment_length must be present");
    let state_diff_size =
        data.remove(constants::STATE_DIFF_SIZE).expect("state_diff_size must be present");
    let n_events = data.remove(constants::N_EVENTS).expect("n_events must be present");
    let sierra_gas = GasAmount(
        data.remove(constants::SIERRA_GAS)
            .expect("sierra_gas must be present")
            .try_into()
            .unwrap_or_else(|err| panic!("Failed to convert 'sierra_gas' into GasAmount: {err}.")),
    );
    let n_txs = data.remove(constants::N_TXS).expect("n_txs must be present");
    let proving_gas = GasAmount(
        data.remove(constants::PROVING_GAS)
            .expect("proving_gas must be present")
            .try_into()
            .unwrap_or_else(|err| panic!("Failed to convert 'proving_gas' into GasAmount: {err}.")),
    );

    assert!(
        data.is_empty(),
        "Unexpected keys in bouncer config 'full_total_weights': {:?}",
        data.keys()
    );

    Ok(BouncerWeights {
        l1_gas,
        message_segment_length,
        state_diff_size,
        n_events,
        n_txs,
        sierra_gas,
        proving_gas,
    })
}

fn hash_map_into_builtin_weights(
    mut data: HashMap<String, usize>,
) -> NativeBlockifierResult<BuiltinWeights> {
    let pedersen = data.remove(Builtin::Pedersen.name()).expect("pedersen must be present");
    let range_check = data.remove(Builtin::RangeCheck.name()).expect("range_check must be present");
    let bitwise = data.remove(Builtin::Bitwise.name()).expect("bitwise must be present");
    let ecdsa = data.remove(Builtin::Ecdsa.name()).expect("ecdsa must be present");
    let keccak = data.remove(Builtin::Keccak.name()).expect("keccak must be present");
    let add_mod = data.remove(Builtin::AddMod.name()).expect("add_mod must be present");
    let mul_mod = data.remove(Builtin::MulMod.name()).expect("mul_mod must be present");
    let ec_op = data.remove(Builtin::EcOp.name()).expect("ec_op must be present");
    let range_check96 =
        data.remove(Builtin::RangeCheck96.name()).expect("range_check96 must be present");
    let poseidon = data.remove(Builtin::Poseidon.name()).expect("poseidon must be present");

    assert!(data.is_empty(), "Unexpected keys in builtin weights: {:?}", data.keys());

    Ok(BuiltinWeights {
        pedersen,
        range_check,
        bitwise,
        ecdsa,
        keccak,
        add_mod,
        mul_mod,
        ec_op,
        range_check96,
        poseidon,
    })
}

#[derive(Debug, Default, FromPyObject)]
pub struct PyConcurrencyConfig {
    pub enabled: bool,
    pub n_workers: usize,
    pub chunk_size: usize,
}

impl From<PyConcurrencyConfig> for ConcurrencyConfig {
    fn from(py_concurrency_config: PyConcurrencyConfig) -> Self {
        ConcurrencyConfig {
            enabled: py_concurrency_config.enabled,
            n_workers: py_concurrency_config.n_workers,
            chunk_size: py_concurrency_config.chunk_size,
        }
    }
}
#[derive(Clone, Debug, Default, FromPyObject)]
pub struct PySierraCompilationConfig {
    pub sierra_to_native_compiler_path: String,
    pub max_native_bytecode_size: u64,
    pub max_cpu_time: u64,
    pub max_memory_usage: u64,
    pub optimization_level: u8,
}

impl From<PySierraCompilationConfig> for SierraCompilationConfig {
    fn from(py_sierra_compilation_config: PySierraCompilationConfig) -> Self {
        SierraCompilationConfig {
            compiler_binary_path: if py_sierra_compilation_config
                .sierra_to_native_compiler_path
                .is_empty()
            {
                None
            } else {
                Some(PathBuf::from(py_sierra_compilation_config.sierra_to_native_compiler_path))
            },
            max_file_size: Some(py_sierra_compilation_config.max_native_bytecode_size),
            max_cpu_time: Some(py_sierra_compilation_config.max_cpu_time),
            max_memory_usage: Some(py_sierra_compilation_config.max_memory_usage),
            optimization_level: py_sierra_compilation_config.optimization_level,
        }
    }
}

#[derive(Clone, Debug, FromPyObject)]
pub struct PyCairoNativeRunConfig {
    pub run_cairo_native: bool,
    pub wait_on_native_compilation: bool,
    pub channel_size: usize,
    // Determines which contracts are allowd to run Cairo Native. `None` → All.
    pub native_classes_whitelist: Option<Vec<PyFelt>>,
    pub panic_on_compilation_failure: bool,
}

impl Default for PyCairoNativeRunConfig {
    fn default() -> Self {
        Self {
            run_cairo_native: false,
            wait_on_native_compilation: false,
            channel_size: DEFAULT_COMPILATION_REQUEST_CHANNEL_SIZE,
            native_classes_whitelist: None,
            panic_on_compilation_failure: false,
        }
    }
}

impl From<PyCairoNativeRunConfig> for CairoNativeRunConfig {
    fn from(py_cairo_native_run_config: PyCairoNativeRunConfig) -> Self {
        let native_classes_whitelist = match py_cairo_native_run_config.native_classes_whitelist {
            Some(felts) => NativeClassesWhitelist::Limited(
                felts.into_iter().map(|felt| ClassHash(felt.0)).collect(),
            ),
            None => NativeClassesWhitelist::All,
        };

        CairoNativeRunConfig {
            run_cairo_native: py_cairo_native_run_config.run_cairo_native,
            wait_on_native_compilation: py_cairo_native_run_config.wait_on_native_compilation,
            channel_size: py_cairo_native_run_config.channel_size,
            native_classes_whitelist,
            panic_on_compilation_failure: py_cairo_native_run_config.panic_on_compilation_failure,
        }
    }
}

#[derive(Debug, Clone, FromPyObject)]
pub struct PyContractClassManagerConfig {
    pub contract_cache_size: usize,
    pub cairo_native_run_config: PyCairoNativeRunConfig,
    pub native_compiler_config: PySierraCompilationConfig,
}

impl Default for PyContractClassManagerConfig {
    fn default() -> Self {
        Self {
            contract_cache_size: GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST,
            cairo_native_run_config: PyCairoNativeRunConfig::default(),
            native_compiler_config: PySierraCompilationConfig::default(),
        }
    }
}

impl From<PyContractClassManagerConfig> for ContractClassManagerConfig {
    fn from(py_contract_class_manager_config: PyContractClassManagerConfig) -> Self {
        ContractClassManagerConfig {
            contract_cache_size: py_contract_class_manager_config.contract_cache_size,
            cairo_native_run_config: py_contract_class_manager_config
                .cairo_native_run_config
                .into(),
            native_compiler_config: py_contract_class_manager_config.native_compiler_config.into(),
        }
    }
}

#[pyclass]
#[derive(Clone, Default)]
pub struct PyCompiledClassHashesForMigration {
    #[pyo3(get)]
    pub compiled_class_hash_v2_to_v1: Vec<(PyFelt, PyFelt)>,
}

impl From<CompiledClassHashesForMigration> for PyCompiledClassHashesForMigration {
    fn from(hashes: CompiledClassHashesForMigration) -> Self {
        Self {
            compiled_class_hash_v2_to_v1: hashes
                .into_iter()
                .map(|(v2_hash, v1_hash)| (PyFelt(v2_hash.0), PyFelt(v1_hash.0)))
                .collect(),
        }
    }
}
