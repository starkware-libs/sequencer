//! In-memory proof verification using cairo-air.

use std::sync::Arc;

use cairo_air::utils::{get_verification_output, to_cairo_proof, VerificationOutput};
use cairo_air::verifier::verify_cairo;
use cairo_air::{CairoProofSorted, PreProcessedTraceVariant};
use proving_utils::proof_encoding::{ProofBytes, ProofEncodingError};
use starknet_api::transaction::fields::{ProgramOutput, Proof};
use starknet_types_core::felt::{Felt, FromStrError};
use stwo::core::vcs_lifted::blake2_merkle::{Blake2sMerkleChannel, Blake2sMerkleHasher};
use thiserror::Error;

/// Output from verifying a proof.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyProofOutput {
    /// The raw program output extracted from the proof.
    pub program_output: ProgramOutput,
    /// The program hash extracted from the proof.
    pub program_hash: Felt,
}

#[derive(Error, Debug)]
pub enum VerifyProofAndFactsError {
    #[error("Proof is empty.")]
    EmptyProof,
    #[error("Proof facts do not match proof output.")]
    ProofFactsMismatch,
    #[error("Failed to parse expected bootloader program hash: {0}")]
    ParseExpectedHash(#[source] FromStrError),
    #[error("Bootloader program hash mismatch.")]
    BootloaderHashMismatch,
    #[error("Invalid number of tasks in program output: expected 1, got {0}.")]
    InvalidNumberOfTasks(Felt),
    #[error("Invalid proof version: expected {expected}, got {actual}.")]
    InvalidProofVersion { expected: Felt, actual: Felt },
    #[error(transparent)]
    Verify(#[from] VerifyProofError),
}

impl PartialEq for VerifyProofAndFactsError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::EmptyProof, Self::EmptyProof) => true,
            (Self::ProofFactsMismatch, Self::ProofFactsMismatch) => true,
            (Self::ParseExpectedHash(lhs), Self::ParseExpectedHash(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::BootloaderHashMismatch, Self::BootloaderHashMismatch) => true,
            (Self::InvalidNumberOfTasks(lhs), Self::InvalidNumberOfTasks(rhs)) => lhs == rhs,
            (
                Self::InvalidProofVersion { expected: exp_l, actual: act_l },
                Self::InvalidProofVersion { expected: exp_r, actual: act_r },
            ) => exp_l == exp_r && act_l == act_r,
            (Self::Verify(lhs), Self::Verify(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

#[derive(Error, Debug)]
pub enum VerifyProofError {
    #[error("Failed to encode proof: {0}")]
    EncodeProof(#[from] ProofEncodingError),
    #[error("Failed to deserialize proof: {0}")]
    DeserializeProof(String),
    #[error("Proof verification failed: {0}")]
    Verification(String),
    #[error("Failed to convert verification output: {0}")]
    OutputConversion(String),
}

impl PartialEq for VerifyProofError {
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

pub fn verify_proof(proof: Proof) -> Result<VerifyProofOutput, VerifyProofError> {
    // Convert proof to raw bytes.
    let proof_bytes = ProofBytes::try_from(proof)?;

    // Deserialize proof from bincode format (using bincode v1 API).
    let cairo_proof_sorted: CairoProofSorted<Blake2sMerkleHasher> =
        bincode::deserialize(&proof_bytes.0)
            .map_err(|e| VerifyProofError::DeserializeProof(e.to_string()))?;

    // Extract verification output from the proof's public memory.
    let verification_output =
        get_verification_output(&cairo_proof_sorted.claim.public_data.public_memory);

    // Convert CairoProofSorted to CairoProof for verification.
    let preprocessed_trace = PreProcessedTraceVariant::Canonical;
    let cairo_proof = to_cairo_proof(cairo_proof_sorted, preprocessed_trace);

    // Verify the proof.
    verify_cairo::<Blake2sMerkleChannel>(cairo_proof, preprocessed_trace)
        .map_err(|e| VerifyProofError::Verification(format!("{e:?}")))?;

    // Convert starknet_ff::FieldElement values to starknet_types_core::felt::Felt.
    let output = convert_verification_output_to_felts(&verification_output)?;
    let program_output = ProgramOutput(Arc::new(output));
    let program_hash = felt_from_starknet_ff(verification_output.program_hash);

    Ok(VerifyProofOutput { program_output, program_hash })
}

/// Converts cairo-air VerificationOutput output field to a Vec of Felt.
fn convert_verification_output_to_felts(
    output: &VerificationOutput,
) -> Result<Vec<Felt>, VerifyProofError> {
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
