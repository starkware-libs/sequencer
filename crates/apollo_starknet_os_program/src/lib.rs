use std::collections::HashMap;
use std::sync::LazyLock;

use cairo_vm::types::program::Program;

use crate::program_hash::ProgramHashes;

#[cfg(test)]
mod constants_test;
pub mod os_code_snippets;
pub mod program_hash;
#[cfg(feature = "test_programs")]
pub mod test_programs;

pub static CAIRO_FILES_MAP: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/cairo_files_map.json")))
        .unwrap_or_else(|error| panic!("Failed to deserialize cairo_files_map.json: {error:?}."))
});

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

pub static PROGRAM_HASHES: LazyLock<ProgramHashes> = LazyLock::new(|| {
    // As the program hash file may not exist at runtime, it's contents must be included at compile
    // time.
    serde_json::from_str(include_str!("program_hash.json"))
        .expect("Failed to deserialize program_hash.json.")
});
