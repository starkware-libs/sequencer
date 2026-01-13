//! In-memory proof verification using cairo-air.

use proving_utils::proof_encoding::{ProofBytes, ProofEncodingError};
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_types_core::felt::{Felt, FromStrError};
use thiserror::Error;

/// Output from verifying a proof.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyProofOutput {
    /// The proof facts extracted from the proof.
    pub proof_facts: ProofFacts,
    /// The program hash extracted from the proof.
    pub program_hash: Felt,
}

/// Errors that can occur during proof verification.
#[derive(Error, Debug)]
pub enum VerifyProofError {
    #[error("Proof is empty.")]
    EmptyProof,
    #[error("Proof facts do not match proof output.")]
    ProofFactsMismatch,
    #[error("Failed to parse expected bootloader program hash: {0}")]
    ParseExpectedHash(#[source] FromStrError),
    #[error("Bootloader program hash mismatch.")]
    BootloaderHashMismatch,
    #[error(transparent)]
    InMemoryVerify(#[from] InMemoryVerifyError),
}

impl PartialEq for VerifyProofError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::EmptyProof, Self::EmptyProof) => true,
            (Self::ProofFactsMismatch, Self::ProofFactsMismatch) => true,
            (Self::ParseExpectedHash(lhs), Self::ParseExpectedHash(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::BootloaderHashMismatch, Self::BootloaderHashMismatch) => true,
            (Self::InMemoryVerify(lhs), Self::InMemoryVerify(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

/// Errors that can occur when verifying a proof in memory.
#[derive(Error, Debug)]
pub enum InMemoryVerifyError {
    #[error("Failed to encode proof: {0}")]
    EncodeProof(#[from] ProofEncodingError),
    #[error("Failed to deserialize proof: {0}")]
    DeserializeProof(String),
    #[error("Proof verification failed: {0}")]
    Verification(String),
    #[error("Failed to convert verification output: {0}")]
    OutputConversion(String),
}

impl PartialEq for InMemoryVerifyError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::EncodeProof(lhs), Self::EncodeProof(rhs)) => lhs.to_string() == rhs.to_string(),
            (Self::DeserializeProof(lhs), Self::DeserializeProof(rhs)) => lhs == rhs,
            (Self::Verification(lhs), Self::Verification(rhs)) => lhs == rhs,
            (Self::OutputConversion(lhs), Self::OutputConversion(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

/// Verifies a proof in memory using cairo-air directly.
///
/// This function performs proof verification without subprocess spawning or temp file I/O:
/// - Decodes the proof from the packed representation to raw bytes
/// - Deserializes the proof using bincode
/// - Extracts verification output (proof facts and program hash)
/// - Verifies the proof using cairo-air's verifier
///
/// Returns the verified proof facts and program hash as in-memory objects.
pub fn verify_proof(proof: Proof) -> Result<VerifyProofOutput, InMemoryVerifyError> {
    use std::sync::Arc;

    use cairo_air::CairoProofSorted;
    use stwo::core::vcs_lifted::blake2_merkle::{Blake2sMerkleChannel, Blake2sMerkleHasher};

    // Convert proof to raw bytes.
    let proof_bytes = ProofBytes::try_from(proof)?;

    // Deserialize proof from bincode format (using bincode v1 API).
    // Note: The prover serializes CairoProofSorted, not CairoProof.
    let cairo_proof_sorted: CairoProofSorted<Blake2sMerkleHasher> =
        bincode::deserialize(&proof_bytes.0)
            .map_err(|e| InMemoryVerifyError::DeserializeProof(e.to_string()))?;

    // Extract verification output from the proof's public memory.
    let verification_output = cairo_air::utils::get_verification_output(
        &cairo_proof_sorted.claim.public_data.public_memory,
    );

    // Convert CairoProofSorted to CairoProof for verification.
    let preprocessed_trace = cairo_air::PreProcessedTraceVariant::Canonical;
    let cairo_proof =
        cairo_air::utils::to_cairo_proof(cairo_proof_sorted, preprocessed_trace.clone());

    // Verify the proof.
    cairo_air::verifier::verify_cairo::<Blake2sMerkleChannel>(cairo_proof, preprocessed_trace)
        .map_err(|e| InMemoryVerifyError::Verification(format!("{e:?}")))?;

    // Convert starknet_ff::FieldElement values to starknet_types_core::felt::Felt.
    let facts = convert_verification_output_to_felts(&verification_output)?;
    let proof_facts = ProofFacts(Arc::new(facts));
    let program_hash = felt_from_starknet_ff(verification_output.program_hash);

    Ok(VerifyProofOutput { proof_facts, program_hash })
}

/// Converts cairo-air VerificationOutput output field to a Vec of Felt.
fn convert_verification_output_to_felts(
    output: &cairo_air::utils::VerificationOutput,
) -> Result<Vec<Felt>, InMemoryVerifyError> {
    let mut facts = Vec::new();
    for fact in &output.output {
        facts.push(felt_from_starknet_ff(*fact));
    }
    Ok(facts)
}

/// Converts a starknet_ff::FieldElement to starknet_types_core::felt::Felt.
fn felt_from_starknet_ff(fe: starknet_ff::FieldElement) -> Felt {
    let bytes = fe.to_bytes_be();
    Felt::from_bytes_be(&bytes)
}
