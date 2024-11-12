use std::collections::HashMap;

use blockifier::abi::constants;
use blockifier::blockifier::config::{CairoNativeConfig, ConcurrencyConfig};
use blockifier::bouncer::{BouncerConfig, BouncerWeights, BuiltinCount, HashMapWrapper};
use blockifier::state::global_cache::GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST;
use blockifier::versioned_constants::VersionedConstantsOverrides;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use pyo3::prelude::*;

use crate::errors::{
    InvalidNativeBlockifierInputError,
    NativeBlockifierError,
    NativeBlockifierInputError,
    NativeBlockifierResult,
};

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

// From Python to Rust.

#[pyclass]
#[derive(Clone)]
pub struct PyVersionedConstantsOverrides {
    pub validate_max_n_steps: u32,
    pub max_recursion_depth: usize,
    pub invoke_tx_max_n_steps: u32,
}

#[pymethods]
impl PyVersionedConstantsOverrides {
    #[new]
    #[pyo3(signature = (validate_max_n_steps, max_recursion_depth, invoke_tx_max_n_steps))]
    pub fn create(
        validate_max_n_steps: u32,
        max_recursion_depth: usize,
        invoke_tx_max_n_steps: u32,
    ) -> Self {
        Self { validate_max_n_steps, max_recursion_depth, invoke_tx_max_n_steps }
    }
}

impl From<PyVersionedConstantsOverrides> for VersionedConstantsOverrides {
    fn from(py_versioned_constants_overrides: PyVersionedConstantsOverrides) -> Self {
        let PyVersionedConstantsOverrides {
            validate_max_n_steps,
            max_recursion_depth,
            invoke_tx_max_n_steps,
        } = py_versioned_constants_overrides;
        Self { validate_max_n_steps, max_recursion_depth, invoke_tx_max_n_steps }
    }
}

#[derive(Clone, Debug, FromPyObject)]
pub struct PyBouncerConfig {
    pub full_total_weights: HashMap<String, usize>,
}

impl TryFrom<PyBouncerConfig> for BouncerConfig {
    type Error = NativeBlockifierError;
    fn try_from(py_bouncer_config: PyBouncerConfig) -> Result<Self, Self::Error> {
        Ok(BouncerConfig {
            block_max_capacity: hash_map_into_bouncer_weights(
                py_bouncer_config.full_total_weights.clone(),
            )?,
        })
    }
}

fn hash_map_into_builtin_count(
    builtins: HashMap<String, usize>,
) -> Result<BuiltinCount, NativeBlockifierInputError> {
    let mut wrapper = HashMapWrapper::new();
    for (builtin_name, count) in builtins.iter() {
        let builtin = BuiltinName::from_str_with_suffix(builtin_name)
            .ok_or(NativeBlockifierInputError::UnknownBuiltin(builtin_name.clone()))?;
        wrapper.insert(builtin, *count);
    }
    let builtin_count: BuiltinCount = wrapper.into();
    if builtin_count.all_non_zero() {
        Ok(builtin_count)
    } else {
        Err(NativeBlockifierInputError::InvalidNativeBlockifierInputError(
            InvalidNativeBlockifierInputError::InvalidBuiltinCounts(builtin_count),
        ))
    }
}

fn hash_map_into_bouncer_weights(
    mut data: HashMap<String, usize>,
) -> NativeBlockifierResult<BouncerWeights> {
    let gas = data.remove(constants::L1_GAS_USAGE).expect("gas_weight must be present");
    let n_steps = data.remove(constants::N_STEPS_RESOURCE).expect("n_steps must be present");
    let message_segment_length = data
        .remove(constants::MESSAGE_SEGMENT_LENGTH)
        .expect("message_segment_length must be present");
    let state_diff_size =
        data.remove(constants::STATE_DIFF_SIZE).expect("state_diff_size must be present");
    let n_events = data.remove(constants::N_EVENTS).expect("n_events must be present");
    Ok(BouncerWeights {
        gas,
        n_steps,
        message_segment_length,
        state_diff_size,
        n_events,
        builtin_count: hash_map_into_builtin_count(data)?,
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

#[derive(Debug, Clone, Copy, FromPyObject)]
pub struct PyCairoNativeConfig {
    pub run_cairo_native: bool,
    pub block_compilation: bool,
    pub global_contract_cache_size: usize,
}

impl PyCairoNativeConfig {
    pub fn default() -> Self {
        Self {
            run_cairo_native: false,
            block_compilation: false,
            global_contract_cache_size: GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST,
        }
    }
}

impl From<PyCairoNativeConfig> for CairoNativeConfig {
    fn from(py_cairo_native_config: PyCairoNativeConfig) -> Self {
        CairoNativeConfig {
            run_cairo_native: py_cairo_native_config.run_cairo_native,
            block_compilation: py_cairo_native_config.block_compilation,
            global_contract_cache_size: py_cairo_native_config.global_contract_cache_size,
        }
    }
}
