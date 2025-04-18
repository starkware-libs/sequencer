use std::fs::File;
use std::sync::LazyLock;

use cairo_vm::types::program::Program;

use crate::program_hash::{ProgramHash, PROGRAM_HASH_PATH};

pub mod program_hash;
#[cfg(any(test, feature = "testing"))]
pub mod test_utils;

pub const OS_PROGRAM_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/starknet_os_bytes"));
pub const AGGREGATOR_PROGRAM_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/starknet_aggregator_bytes"));

pub static OS_PROGRAM: LazyLock<Program> = LazyLock::new(|| {
    Program::from_bytes(OS_PROGRAM_BYTES, Some("main")).expect("Failed to load the OS bytes.")
});
pub static AGGREGATOR_PROGRAM: LazyLock<Program> = LazyLock::new(|| {
    Program::from_bytes(AGGREGATOR_PROGRAM_BYTES, Some("main"))
        .expect("Failed to load the aggregator bytes.")
});

pub static PROGRAM_HASH: LazyLock<ProgramHash> = LazyLock::new(|| {
    serde_json::from_reader(
        File::open(&*PROGRAM_HASH_PATH)
            .unwrap_or_else(|error| panic!("Failed to open {PROGRAM_HASH_PATH:?}: {error:?}.")),
    )
    .unwrap_or_else(|error| panic!("Failed to deserialize {PROGRAM_HASH_PATH:?}: {error:?}."))
});
