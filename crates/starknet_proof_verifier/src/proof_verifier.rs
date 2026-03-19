//! Proof verification using privacy_circuit_verify.

use std::sync::Arc;

use apollo_sizeof::SizeOf;
use serde::{Deserialize, Serialize};
use starknet_api::transaction::fields::{Proof, ProofFacts, PROOF_VERSION};
use starknet_types_core::felt::Felt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VerifyProofError {
    #[error("Proof is empty.")]
    EmptyProof,
    #[error(transparent)]
    ProgramOutputError(#[from] ProgramOutputError),
    #[error("Invalid proof version: expected {expected}, got {actual}.")]
    InvalidProofVersion { expected: Felt, actual: Felt },
    #[error("Proof facts too short: expected at least 3 elements, got {length}.")]
    ProofFactsTooShort { length: usize },
    // TODO(AvivG): Remove this once PrivacyProofOutput holds u8 proof.
    #[error("Proof byte length {0} is not a multiple of 4.")]
    InvalidProofLength(usize),
    #[error("Proof verification failed: {0}")]
    Verification(String),
}

impl PartialEq for VerifyProofError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::EmptyProof, Self::EmptyProof) => true,
            (Self::ProgramOutputError(lhs), Self::ProgramOutputError(rhs)) => lhs == rhs,
            (
                Self::InvalidProofVersion { expected: exp_l, actual: act_l },
                Self::InvalidProofVersion { expected: exp_r, actual: act_r },
            ) => exp_l == exp_r && act_l == act_r,
            (Self::InvalidProofLength(lhs), Self::InvalidProofLength(rhs)) => lhs == rhs,
            (Self::Verification(lhs), Self::Verification(rhs)) => lhs == rhs,
            (Self::ProofFactsTooShort { length: l }, Self::ProofFactsTooShort { length: r }) => {
                l == r
            }
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

/// Reconstructs the output preimage from proof facts for circuit verification.
///
/// Proof facts layout: `[PROOF_VERSION, variant, program_hash, ...task_output]`
/// Output preimage layout: `[num_tasks=1, output_size, program_hash, ...task_output]`
/// where `output_size = task_content.len() + 1` (includes itself).
pub fn reconstruct_output_preimage(
    proof_facts: &ProofFacts,
) -> Result<Vec<Felt>, VerifyProofError> {
    // Proof facts must contain at least [PROOF_VERSION, variant, program_hash].
    if proof_facts.0.len() < 3 {
        return Err(VerifyProofError::ProofFactsTooShort { length: proof_facts.0.len() });
    }
    // Skip PROOF_VERSION (index 0) and variant (index 1).
    let task_content = &proof_facts.0[2..];
    let output_size = Felt::from(
        u64::try_from(task_content.len() + 1).expect("task content length exceeds u64::MAX"),
    );
    Ok([Felt::ONE, output_size].into_iter().chain(task_content.iter().copied()).collect())
}

/// Verifies a submitted proof against the proof facts using the circuit verifier.
pub fn verify_proof(proof_facts: ProofFacts, proof: Proof) -> Result<(), VerifyProofError> {
    // Reject empty proof payloads before running the verifier.
    if proof.is_empty() {
        return Err(VerifyProofError::EmptyProof);
    }

    // Validate that the first element of proof facts is PROOF_VERSION.
    let expected_proof_version = PROOF_VERSION;
    let actual_first = proof_facts.0.first().copied().unwrap_or_default();
    if actual_first != expected_proof_version {
        return Err(VerifyProofError::InvalidProofVersion {
            expected: expected_proof_version,
            actual: actual_first,
        });
    }

    // Reconstruct the output preimage from proof facts and verify the proof.
    let output_preimage = reconstruct_output_preimage(&proof_facts)?;
    // TODO(AvivG): this conversion is temporary until PrivacyProofOutput holds u8 proof.
    let (chunks, []) = proof.0.as_chunks::<4>() else {
        return Err(VerifyProofError::InvalidProofLength(proof.0.len()));
    };
    let proof_u32s: Vec<u32> = chunks.iter().map(|c| u32::from_be_bytes(*c)).collect();
    let proof_output =
        privacy_circuit_verify::PrivacyProofOutput { proof: proof_u32s, output_preimage };
    privacy_circuit_verify::verify_recursive_circuit(&proof_output)
        .map_err(|e| VerifyProofError::Verification(e.to_string()))?;

    Ok(())
}
