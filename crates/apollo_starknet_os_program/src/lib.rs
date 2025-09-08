use std::collections::HashMap;
use std::sync::LazyLock;

use cairo_vm::types::program::Program;
use starknet_api::core::L2_ADDRESS_UPPER_BOUND;
use starknet_types_core::felt::Felt;

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
    let mut program =
        Program::from_bytes(OS_PROGRAM_BYTES, Some("main")).expect("Failed to load the OS bytes.");
    program.constants.extend(EXTRA_OS_CONSTANTS.clone());
    program
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

static EXTRA_OS_CONSTANTS: LazyLock<HashMap<String, Felt>> = LazyLock::new(|| {
    [
        (
            "starkware.cairo.common.cairo_keccak.keccak.KECCAK_STATE_SIZE_FELTS".to_string(),
            Felt::from(25_u128),
        ),
        (
            "starkware.cairo.common.cairo_keccak.packed_keccak.BLOCK_SIZE".to_string(),
            Felt::from(3_u128),
        ),
        (
            "starkware.starknet.common.storage.ADDR_BOUND".to_string(),
            Felt::from(*L2_ADDRESS_UPPER_BOUND),
        ),
        ("starkware.cairo.common.cairo_secp.constants.BETA".to_string(), Felt::from(7_u128)),
        (
            "starkware.cairo.common.math.assert_250_bit.UPPER_BOUND".to_string(),
            Felt::TWO.pow(250_u128),
        ),
        ("starkware.cairo.common.math.assert_250_bit.SHIFT".to_string(), Felt::TWO.pow(128_u128)),
        (
            "starkware.cairo.common.math.split_felt.MAX_HIGH".to_string(),
            (-Felt::ONE)
                * (Felt::TWO.pow(128_u128).inverse().expect("Failed to get inverse of 2**128.")),
        ),
        ("starkware.cairo.common.math.split_felt.MAX_LOW".to_string(), Felt::ZERO),
        ("starkware.cairo.common.cairo_secp.constants.BASE".to_string(), Felt::TWO.pow(86_u128)),
        (
            "starkware.cairo.common.math.assert_le_felt.PRIME_OVER_3_HIGH".to_string(),
            Felt::from(0x2aaaaaaaaaaaab05555555555555556_u128),
        ),
        (
            "starkware.cairo.common.math.assert_le_felt.PRIME_OVER_2_HIGH".to_string(),
            Felt::from(0x4000000000000088000000000000001_u128),
        ),
        (
            "starkware.cairo.common.cairo_keccak.keccak.BYTES_IN_WORD".to_string(),
            Felt::from(8_u128),
        ),
        (
            "starkware.cairo.common.cairo_keccak.keccak.KECCAK_FULL_RATE_IN_BYTES".to_string(),
            Felt::from(136_u128),
        ),
        (
            "starkware.cairo.common.builtin_keccak.keccak.KECCAK_FULL_RATE_IN_BYTES".to_string(),
            Felt::from(136_u128),
        ),
        ("starkware.cairo.common.uint256.SHIFT".to_string(), Felt::TWO.pow(128_u128)),
        (
            "starkware.cairo.common.cairo_blake2s.packed_blake2s.N_PACKED_INSTANCES".to_string(),
            Felt::from(7_u128),
        ),
        (
            "starkware.cairo.common.cairo_blake2s.packed_blake2s.INPUT_BLOCK_FELTS".to_string(),
            Felt::from(16_u128),
        ), // renamed from BLAKE2S_INPUT_CHUNK_SIZE_FELTS
        (
            "starkware.cairo.common.cairo_sha256.sha256_utils.SHA256_INPUT_CHUNK_SIZE_FELTS"
                .to_string(),
            Felt::from(16_u128),
        ),
        (
            "starkware.cairo.common.cairo_sha256.sha256_utils.SHA256_STATE_SIZE_FELTS".to_string(),
            Felt::from(8_u128),
        ),
        (
            "starkware.cairo.common.cairo_sha256.sha256_utils.BATCH_SIZE".to_string(),
            Felt::from(7_u128),
        ),
    ]
    .into()
});
