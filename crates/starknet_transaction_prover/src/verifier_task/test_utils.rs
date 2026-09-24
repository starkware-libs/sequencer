use std::io::Read;

use flate2::read::GzDecoder;
use starknet_types_core::felt::Felt;

use crate::verifier_task::{
    run_circuit_verifier_task,
    CircuitVerifierTaskOutput,
    VerifierTaskError,
};

pub const SIMPLE_BOOTLOADER_PROGRAM_GZ: &[u8] =
    include_bytes!("../../resources/simple_bootloader_compiled.json.gz");

pub const VERIFIER_EXECUTABLE_GZ: &[u8] =
    include_bytes!("../../resources/stwo_circuit_verifier_canonical_small.executable.json.gz");

pub const PROCESSED_PROOF_GZ: &[u8] = include_bytes!("../../resources/one_leaf_root_proof.json.gz");

pub const FIXTURE_VERIFIER_PROGRAM_HASH: Felt =
    Felt::from_hex_unchecked("0x764dc214c7f45a6899b05d42ba4d23d5849e0bf0383ec951f000b1742107fdd");

pub fn gunzip(compressed_bytes: &[u8]) -> Vec<u8> {
    let mut decompressed_bytes = Vec::new();
    GzDecoder::new(compressed_bytes).read_to_end(&mut decompressed_bytes).unwrap();
    decompressed_bytes
}

pub fn processed_proof_felts() -> Vec<Felt> {
    let processed_proof_hex: Vec<String> =
        serde_json::from_slice(&gunzip(PROCESSED_PROOF_GZ)).unwrap();
    processed_proof_hex
        .iter()
        .map(|proof_felt_hex| Felt::from_hex(proof_felt_hex).unwrap())
        .collect()
}

pub fn one_leaf_proof_facts() -> Vec<Felt> {
    [
        Felt::ZERO,
        Felt::ZERO,
        Felt::from_hex("0x32b88272d54b83880ebebd9c4292a650bee27d1575e82123391b6df2932e843")
            .unwrap(),
        Felt::from_hex("0xb").unwrap(),
        Felt::from_hex("0xd").unwrap(),
        Felt::from_hex("0x11").unwrap(),
    ]
    .to_vec()
}

pub fn run_one_leaf_verifier_task() -> Result<CircuitVerifierTaskOutput, VerifierTaskError> {
    run_circuit_verifier_task(
        &gunzip(SIMPLE_BOOTLOADER_PROGRAM_GZ),
        &gunzip(VERIFIER_EXECUTABLE_GZ),
        &processed_proof_felts(),
    )
}
