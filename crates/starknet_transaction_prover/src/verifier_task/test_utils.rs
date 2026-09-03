//! The vendored verifier-task fixtures - the simple bootloader, the circuit verifier
//! executable, and the `four_leaves` golden root proof - with helpers to run them.

use std::io::Read;

use flate2::read::GzDecoder;
use starknet_types_core::felt::Felt;

use super::{run_circuit_verifier_task, CircuitVerifierTaskOutput, VerifierTaskError};

/// The simple bootloader compiled program, from cairo-program-runner-lib at
/// proving-utils tag v0.14.3-rust-bump (the pinned workspace dependency),
/// resources/compiled_programs/bootloaders/simple_bootloader_compiled.json.
pub const SIMPLE_BOOTLOADER_PROGRAM_GZ: &[u8] =
    include_bytes!("../../resources/simple_bootloader_compiled.json.gz");

/// The circuit verifier executable, built with scarb 2.19.4 from proving-dev commit
/// b75d21f91fe846401e002ad169dbcbe57f289ebf: `scarb build --package
/// stwo_circuit_verifier` (dev profile, default features). Its multiverifier constants
/// match the `canonical_small` registry the fold's circuit hash constants are pinned to.
pub const VERIFIER_EXECUTABLE_GZ: &[u8] =
    include_bytes!("../../resources/stwo_circuit_verifier_canonical_small.executable.json.gz");

/// The `four_leaves` golden root proof (proving-dev commit b75d21f9,
/// crates/stwo_run_and_prove_recursive_tree/test_data/goldens/four_leaves/root.proof):
/// the recursive tree over four identical leaves, as the verifier's felt252 argument
/// stream (a JSON array of hex strings).
pub const ROOT_PROOF_GZ: &[u8] = include_bytes!("../../resources/four_leaves_root_proof.json.gz");

/// The fixture verifier executable's hash under the Blake program hash function, as the
/// simple bootloader outputs it.
pub const FIXTURE_VERIFIER_PROGRAM_HASH: Felt =
    Felt::from_hex_unchecked("0x764dc214c7f45a6899b05d42ba4d23d5849e0bf0383ec951f000b1742107fdd");

pub fn gunzip(compressed_bytes: &[u8]) -> Vec<u8> {
    let mut decompressed_bytes = Vec::new();
    GzDecoder::new(compressed_bytes).read_to_end(&mut decompressed_bytes).unwrap();
    decompressed_bytes
}

pub fn root_proof_felts() -> Vec<Felt> {
    let root_proof_hex: Vec<String> = serde_json::from_slice(&gunzip(ROOT_PROOF_GZ)).unwrap();
    root_proof_hex.iter().map(|proof_felt_hex| Felt::from_hex(proof_felt_hex).unwrap()).collect()
}

/// The proof facts of each of the `four_leaves` fixture's transactions: the fixture's
/// leaf output preimage (see `test_four_leaf_fold_matches_proving_side_golden` in
/// starknet_os), with the two version markers the OS leaf preimage drops prepended.
pub fn four_leaves_proof_facts() -> Vec<Felt> {
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

/// Runs the circuit verifier on the `four_leaves` root proof as a single
/// simple-bootloader task and returns the parsed output page.
pub fn run_four_leaves_verifier_task() -> Result<CircuitVerifierTaskOutput, VerifierTaskError> {
    run_circuit_verifier_task(
        &gunzip(SIMPLE_BOOTLOADER_PROGRAM_GZ),
        &gunzip(VERIFIER_EXECUTABLE_GZ),
        &root_proof_felts(),
    )
}
