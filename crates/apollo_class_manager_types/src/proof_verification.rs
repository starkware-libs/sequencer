//! Helper for running stwo_verify on a proof and extracting the verified output.

use proving_utils::proof_encoding::{ProofBytes, ProofEncodingError};
use proving_utils::stwo_verify::{
    run_stwo_verify,
    ProofFormat,
    StwoVerifyConfig,
    StwoVerifyError,
    StwoVerifyInput,
};
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_types_core::felt::{Felt, FromStrError};
use tempfile::NamedTempFile;
use thiserror::Error;

/// Output from running stwo_verify on a proof.
#[derive(Debug, Clone, PartialEq)]
pub struct StwoVerifyOutput {
    /// The proof facts extracted from the proof.
    pub proof_facts: ProofFacts,
    /// The program hash extracted from the proof.
    pub program_hash: Felt,
}

/// Errors that can occur when running stwo_verify.
#[derive(Error, Debug)]
pub enum RunStwoVerifyError {
    #[error("Failed to create temporary proof file: {0}")]
    CreateProofTempFile(#[source] std::io::Error),
    #[error("Failed to encode proof: {0}")]
    EncodeProof(#[from] ProofEncodingError),
    #[error("Failed to create temporary program output file: {0}")]
    CreateProgramOutputTempFile(#[source] std::io::Error),
    #[error("Failed to create temporary program hash file: {0}")]
    CreateProgramHashTempFile(#[source] std::io::Error),
    #[error("Proof verification failed: {0}")]
    InvalidProof(#[from] StwoVerifyError),
    #[error("Failed to read proof facts output: {0}")]
    ReadProgramOutput(#[source] std::io::Error),
    #[error("Failed to parse proof facts output: {0}")]
    ParseProgramOutput(#[source] serde_json::Error),
    #[error("Failed to read program hash output: {0}")]
    ReadProgramHash(#[source] std::io::Error),
    #[error("Failed to parse program hash output: {0}")]
    ParseProgramHash(#[source] FromStrError),
}

impl PartialEq for RunStwoVerifyError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::CreateProofTempFile(lhs), Self::CreateProofTempFile(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::EncodeProof(lhs), Self::EncodeProof(rhs)) => lhs.to_string() == rhs.to_string(),
            (Self::CreateProgramOutputTempFile(lhs), Self::CreateProgramOutputTempFile(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::CreateProgramHashTempFile(lhs), Self::CreateProgramHashTempFile(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::InvalidProof(lhs), Self::InvalidProof(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::ReadProgramOutput(lhs), Self::ReadProgramOutput(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::ParseProgramOutput(lhs), Self::ParseProgramOutput(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::ReadProgramHash(lhs), Self::ReadProgramHash(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            (Self::ParseProgramHash(lhs), Self::ParseProgramHash(rhs)) => {
                lhs.to_string() == rhs.to_string()
            }
            _ => false,
        }
    }
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
    RunStwoVerify(#[from] RunStwoVerifyError),
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
            (Self::RunStwoVerify(lhs), Self::RunStwoVerify(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

/// Runs stwo_verify on a proof and returns the extracted proof facts and program hash.
///
/// This function handles all disk I/O internally:
/// - Writes the proof to a temporary file
/// - Runs stwo_verify to verify the proof
/// - Reads and parses the output files
/// - Returns the verified proof facts and program hash as in-memory objects
pub async fn run_stwo_verify_on_proof(proof: Proof) -> Result<StwoVerifyOutput, RunStwoVerifyError>
{
    // Serialize the proof into a temporary binary file for stwo_verify.
    let proof_bytes = ProofBytes::try_from(proof)?;
    let proof_file = NamedTempFile::new().map_err(RunStwoVerifyError::CreateProofTempFile)?;
    proof_bytes.to_file(proof_file.path())?;

    // Allocate temporary output files for proof facts and the program hash.
    let program_output_file =
        NamedTempFile::new().map_err(RunStwoVerifyError::CreateProgramOutputTempFile)?;
    let program_hash_file =
        NamedTempFile::new().map_err(RunStwoVerifyError::CreateProgramHashTempFile)?;

    // Run stwo_verify and capture its output files.
    let input = StwoVerifyInput {
        proof_path: proof_file.path().to_path_buf(),
        program_output_path: Some(program_output_file.path().to_path_buf()),
        program_hash_output_path: Some(program_hash_file.path().to_path_buf()),
    };
    let config = StwoVerifyConfig { proof_format: ProofFormat::Binary, ..Default::default() };
    run_stwo_verify(&input, &config).await.map_err(RunStwoVerifyError::InvalidProof)?;

    // Decode the proof facts emitted by stwo_verify.
    let extracted_proof_facts_str = std::fs::read_to_string(program_output_file.path())
        .map_err(RunStwoVerifyError::ReadProgramOutput)?;
    let proof_facts: ProofFacts = serde_json::from_str(&extracted_proof_facts_str)
        .map_err(RunStwoVerifyError::ParseProgramOutput)?;

    // Read and parse the program hash output.
    let program_hash_output = std::fs::read_to_string(program_hash_file.path())
        .map_err(RunStwoVerifyError::ReadProgramHash)?;
    let program_hash =
        Felt::from_hex(program_hash_output.trim()).map_err(RunStwoVerifyError::ParseProgramHash)?;

    Ok(StwoVerifyOutput { proof_facts, program_hash })
}
