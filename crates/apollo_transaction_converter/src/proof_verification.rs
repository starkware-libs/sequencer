//! In-memory proof verification using cairo-air.

use std::sync::Arc;

use apollo_sizeof::SizeOf;
use cairo_air::utils::{get_verification_output, to_cairo_proof, VerificationOutput};
use cairo_air::verifier::verify_cairo;
use cairo_air::{CairoProofSorted, PreProcessedTraceVariant};
use proving_utils::proof_encoding::{ProofBytes, ProofEncodingError};
use serde::{Deserialize, Serialize};
use starknet_api::transaction::fields::{Proof, ProofFacts, PROOF_VERSION};
use starknet_types_core::felt::Felt;
use stwo::core::vcs_lifted::blake2_merkle::{Blake2sMerkleChannel, Blake2sMerkleHasher};
use thiserror::Error;

/// Output from verifying a proof using stwo.
#[derive(Debug, Clone, PartialEq)]
pub struct StwoVerifyOutput {
    /// The raw program output extracted from the proof.
    pub program_output: ProgramOutput,
    /// The program hash extracted from the proof.
    pub program_hash: Felt,
}

#[derive(Error, Debug)]
pub enum VerifyProofError {
    #[error("Proof is empty.")]
    EmptyProof,
    #[error("Proof facts do not match proof output.")]
    ProofFactsMismatch,
    #[error(transparent)]
    ProgramOutputError(#[from] ProgramOutputError),
    #[error("Bootloader program hash mismatch.")]
    BootloaderHashMismatch,
    #[error("Invalid proof version: expected {expected}, got {actual}.")]
    InvalidProofVersion { expected: Felt, actual: Felt },
    #[error(transparent)]
    StwoVerify(#[from] StwoVerifyError),
}

impl PartialEq for VerifyProofError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::EmptyProof, Self::EmptyProof) => true,
            (Self::ProofFactsMismatch, Self::ProofFactsMismatch) => true,
            (Self::ProgramOutputError(lhs), Self::ProgramOutputError(rhs)) => lhs == rhs,
            (Self::BootloaderHashMismatch, Self::BootloaderHashMismatch) => true,
            (
                Self::InvalidProofVersion { expected: exp_l, actual: act_l },
                Self::InvalidProofVersion { expected: exp_r, actual: act_r },
            ) => exp_l == exp_r && act_l == act_r,
            (Self::StwoVerify(lhs), Self::StwoVerify(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

#[derive(Error, Debug)]
pub enum StwoVerifyError {
    #[error("Failed to encode proof: {0}")]
    EncodeProof(#[from] ProofEncodingError),
    #[error("Failed to deserialize proof: {0}")]
    DeserializeProof(String),
    #[error("Proof verification failed: {0}")]
    Verification(String),
    #[error("Failed to convert verification output: {0}")]
    OutputConversion(String),
}

impl PartialEq for StwoVerifyError {
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

/// Errors that can occur when converting program output to proof facts.
#[derive(Error, Debug, PartialEq)]
pub enum ProgramOutputError {
    #[error("Program output is empty")]
    Empty,
    #[error("Expected num_tasks to be 1, got {0}")]
    InvalidNumTasks(Felt),
    #[error(
        "Program output too short: expected at least 3 elements (num_tasks, output_size, ...), \
         got {0}"
    )]
    TooShort(usize),
}

/// Raw program output from the bootloader.
/// First element is the number of tasks, followed by the actual output.
#[derive(
    Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, SizeOf,
)]
pub struct ProgramOutput(pub Arc<Vec<Felt>>);

impl ProgramOutput {
    /// Tries to convert ProgramOutput into ProofFacts.
    ///
    /// The bootloader output for a single task is:
    ///   `[num_tasks, output_size, program_hash, ...task_output...]`
    ///
    /// We replace `num_tasks` with `[PROOF_VERSION, program_variant]` and skip `output_size`,
    /// which is a bootloader-internal field not part of the proof facts.
    pub fn try_into_proof_facts(
        &self,
        program_variant: Felt,
    ) -> Result<ProofFacts, ProgramOutputError> {
        let num_tasks = self.0.first().ok_or(ProgramOutputError::Empty)?;
        if *num_tasks != Felt::ONE {
            return Err(ProgramOutputError::InvalidNumTasks(*num_tasks));
        }
        // Need at least: num_tasks, output_size, and at least one task output field.
        if self.0.len() < 3 {
            return Err(ProgramOutputError::TooShort(self.0.len()));
        }
        // Add the proof version and variant markers in place of num_tasks.
        let mut facts = vec![PROOF_VERSION];
        facts.push(program_variant);
        // Skip num_tasks (index 0) and output_size (index 1); add the task output
        // (program_hash followed by the virtual OS output).
        facts.extend_from_slice(&self.0[2..]);
        Ok(ProofFacts(Arc::new(facts)))
    }
}

impl From<Vec<Felt>> for ProgramOutput {
    fn from(value: Vec<Felt>) -> Self {
        Self(Arc::new(value))
    }
}

/// Verifies a proof against the given proof facts.
///
/// Checks that:
/// 1. The proof is non-empty.
/// 2. The proof facts start with the expected proof version.
/// 3. The stwo proof is cryptographically valid.
/// 4. The proof facts match the verified program output.
/// 5. The bootloader program hash matches the expected value.
pub fn verify_proof(
    proof_facts: ProofFacts,
    proof: Proof,
    expected_bootloader_hash: Felt,
) -> Result<(), VerifyProofError> {
    if proof.is_empty() {
        return Err(VerifyProofError::EmptyProof);
    }

    let actual_version = proof_facts.0.first().copied().unwrap_or_default();
    if actual_version != PROOF_VERSION {
        return Err(VerifyProofError::InvalidProofVersion {
            expected: PROOF_VERSION,
            actual: actual_version,
        });
    }

    let output = stwo_verify(proof)?;

    let program_variant = proof_facts.0.get(1).copied().unwrap_or_default();
    let expected_proof_facts = output.program_output.try_into_proof_facts(program_variant)?;
    if expected_proof_facts != proof_facts {
        return Err(VerifyProofError::ProofFactsMismatch);
    }

    if output.program_hash != expected_bootloader_hash {
        return Err(VerifyProofError::BootloaderHashMismatch);
    }

    Ok(())
}

pub fn stwo_verify(proof: Proof) -> Result<StwoVerifyOutput, StwoVerifyError> {
    // Convert proof to raw bytes.
    let proof_bytes = ProofBytes::try_from(proof)?;

    // Deserialize proof from bincode format (using bincode v1 API).
    let cairo_proof_sorted: CairoProofSorted<Blake2sMerkleHasher> =
        bincode::deserialize(&proof_bytes.0)
            .map_err(|e| StwoVerifyError::DeserializeProof(e.to_string()))?;

    // Extract verification output from the proof's public memory.
    let verification_output =
        get_verification_output(&cairo_proof_sorted.claim.public_data.public_memory);

    // Convert CairoProofSorted to CairoProof for verification.
    let preprocessed_trace = PreProcessedTraceVariant::Canonical;
    let cairo_proof = to_cairo_proof(cairo_proof_sorted, preprocessed_trace);

    // Verify the proof.
    verify_cairo::<Blake2sMerkleChannel>(cairo_proof, preprocessed_trace)
        .map_err(|e| StwoVerifyError::Verification(format!("{e:?}")))?;

    // Convert starknet_ff::FieldElement values to starknet_types_core::felt::Felt.
    let output = convert_verification_output_to_felts(&verification_output)?;
    let program_output = ProgramOutput(Arc::new(output));
    let program_hash = felt_from_starknet_ff(verification_output.program_hash);

    Ok(StwoVerifyOutput { program_output, program_hash })
}

/// Converts cairo-air VerificationOutput output field to a Vec of Felt.
fn convert_verification_output_to_felts(
    output: &VerificationOutput,
) -> Result<Vec<Felt>, StwoVerifyError> {
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
