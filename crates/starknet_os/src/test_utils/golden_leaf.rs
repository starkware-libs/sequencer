//! The proving side's golden leaf: its proof facts and digests, its processed proof, and the
//! circuit verifier that verifies the processed proof. The processed proof was generated with
//! `stwo_run_and_prove_recursive_tree` at proving commit `b75d21f9`, under the `canonical_small`
//! circuit registry.

use std::io;

use flate2::read::GzDecoder;
use starknet_types_core::felt::Felt;
use tempfile::NamedTempFile;

use crate::io::os_input::CircuitVerifierTaskInput;
use crate::proof_fact_fold::Blake2sDigestWords;

/// The proof facts of the proving side's golden leaf: two zero version markers, followed by the
/// preimage [program_hash, ...virtual OS output] that the proving side hashes.
pub const GOLDEN_PROOF_FACTS: [Felt; 6] = [
    Felt::ZERO,
    Felt::ZERO,
    Felt::from_hex_unchecked("0x32b88272d54b83880ebebd9c4292a650bee27d1575e82123391b6df2932e843"),
    Felt::from_hex_unchecked("0xb"),
    Felt::from_hex_unchecked("0xd"),
    Felt::from_hex_unchecked("0x11"),
];

pub const GOLDEN_LEAF_OUTPUT_DIGEST: Blake2sDigestWords =
    [1603116091, 3258597502, 2711032228, 4175407283, 343882323, 1898618121, 1344732087, 1064799167];

/// The output digest of the golden leaf's processed proof, where the leaf fills both of the
/// multiverifier's verifier slots.
pub const GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST: Blake2sDigestWords =
    [3336922792, 2174234328, 561756268, 3019198088, 3962216814, 3753317225, 1639677005, 2558993572];

pub const GOLDEN_VERIFICATION_DIGEST: Blake2sDigestWords =
    [2180856259, 1333085512, 862178086, 2311453888, 551146339, 2046676941, 3386628737, 1763131494];

const CIRCUIT_VERIFIER_EXECUTABLE_GZ: &[u8] = include_bytes!(
    "../../resources/circuit_verifier/stwo_circuit_verifier_canonical_small.executable.json.gz"
);
const GOLDEN_PROCESSED_PROOF_GZ: &[u8] =
    include_bytes!("../../resources/circuit_verifier/one_leaf_root_proof.json.gz");

/// The circuit verifier task on the golden leaf's processed proof. Its files stay decompressed for
/// as long as it lives.
pub struct GoldenCircuitVerifierTask {
    executable_file: NamedTempFile,
    processed_proof_file: NamedTempFile,
}

impl GoldenCircuitVerifierTask {
    pub fn decompress() -> Self {
        Self {
            executable_file: gunzip_to_temp_file(CIRCUIT_VERIFIER_EXECUTABLE_GZ),
            processed_proof_file: gunzip_to_temp_file(GOLDEN_PROCESSED_PROOF_GZ),
        }
    }

    pub fn task_input(&self) -> CircuitVerifierTaskInput {
        CircuitVerifierTaskInput {
            executable_path: self.executable_file.path().to_path_buf(),
            processed_proof_path: self.processed_proof_file.path().to_path_buf(),
        }
    }
}

fn gunzip_to_temp_file(compressed_bytes: &[u8]) -> NamedTempFile {
    let mut temp_file = NamedTempFile::new().unwrap();
    io::copy(&mut GzDecoder::new(compressed_bytes), &mut temp_file).unwrap();
    temp_file
}
