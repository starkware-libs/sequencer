use std::collections::BTreeMap;
use std::sync::LazyLock;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

use crate::hint_processor::constants::BUILTIN_INSTANCE_SIZES;

pub mod cairo_dict;
pub mod cairo_runner;
pub mod coverage;
pub mod errors;
#[cfg(test)]
pub mod utils;
pub mod validations;

#[cfg(test)]
#[path = "resource_utils_test.rs"]
mod resource_utils_test;

fn scale_builtin_cells(resources: &ExecutionResources) -> ExecutionResources {
    let mut scaled_resources = resources.clone();
    scaled_resources.builtin_instance_counter = scaled_resources
        .builtin_instance_counter
        .iter()
        .map(|(builtin, count)| (*builtin, count / BUILTIN_INSTANCE_SIZES.get(builtin).unwrap()))
        .collect();
    scaled_resources
}

// Resources consumed by the SHA-256 batch phase, separated into linear and constant factors.
pub const SHA256_BATCH_SIZE: usize = 7;
pub static SHA256_BATCH_RESOURCES_LINEAR_UNSCALED: LazyLock<ExecutionResources> =
    LazyLock::new(|| ExecutionResources {
        n_steps: 11822,
        n_memory_holes: 0,
        builtin_instance_counter: BTreeMap::from([
            (BuiltinName::bitwise, 7800),
            (BuiltinName::range_check, 448),
        ]),
    });
pub static SHA256_BATCH_RESOURCES_LINEAR: LazyLock<ExecutionResources> =
    LazyLock::new(|| scale_builtin_cells(&SHA256_BATCH_RESOURCES_LINEAR_UNSCALED));
pub static SHA256_BATCH_RESOURCES_CONSTANT: LazyLock<ExecutionResources> =
    LazyLock::new(|| ExecutionResources {
        n_steps: 49,
        n_memory_holes: 0,
        builtin_instance_counter: BTreeMap::from([(
            BuiltinName::range_check,
            3 / BUILTIN_INSTANCE_SIZES.get(&BuiltinName::range_check).unwrap(),
        )]),
    });

// Resources consumed by the SHA-512 batch phase, separated into linear and constant factors.
pub const SHA512_BATCH_SIZE: usize = 3;
pub static SHA512_BATCH_RESOURCES_LINEAR_UNSCALED: LazyLock<ExecutionResources> =
    LazyLock::new(|| ExecutionResources {
        n_steps: 13710,
        n_memory_holes: 0,
        builtin_instance_counter: BTreeMap::from([
            (BuiltinName::bitwise, 9960),
            (BuiltinName::range_check, 192),
        ]),
    });
pub static SHA512_BATCH_RESOURCES_LINEAR: LazyLock<ExecutionResources> =
    LazyLock::new(|| scale_builtin_cells(&SHA512_BATCH_RESOURCES_LINEAR_UNSCALED));
pub static SHA512_BATCH_RESOURCES_CONSTANT: LazyLock<ExecutionResources> =
    LazyLock::new(|| ExecutionResources {
        n_steps: 49,
        n_memory_holes: 0,
        builtin_instance_counter: BTreeMap::from([(
            BuiltinName::range_check,
            3 / BUILTIN_INSTANCE_SIZES.get(&BuiltinName::range_check).unwrap(),
        )]),
    });
