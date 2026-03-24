//! Cairo PIE proving using the privacy prover.
//!
//! Provides functionality to generate zero-knowledge proofs from Cairo PIE files.

use std::sync::Arc;

use cairo_vm::vm::runners::cairo_pie::CairoPie;
use privacy_prove::{privacy_recursive_prove, RecursiveProverPrecomputes};
use starknet_api::transaction::fields::Proof;
use starknet_proof_verifier::ProgramOutput;

use crate::errors::ProvingError;

/// Output from the prover containing the proof and associated program output.
#[derive(Debug, Clone)]
pub(crate) struct ProverOutput {
    /// The proof packed as bytes.
    pub proof: Proof,
    /// Raw program output from the bootloader (first element is number of tasks).
    pub program_output: ProgramOutput,
}

/// Proves a Cairo PIE using the privacy recursive prover.
///
/// Calls `privacy_recursive_prove` with the CairoPie and precomputed data on a blocking thread.
pub(crate) async fn prove(
    cairo_pie: CairoPie,
    precomputes: Arc<RecursiveProverPrecomputes>,
) -> Result<ProverOutput, ProvingError> {
    let proof_output = tokio::task::spawn_blocking(move || {
        privacy_recursive_prove(cairo_pie, precomputes).map_err(|e| e.to_string())
    })
    .await
    .map_err(ProvingError::TaskJoin)?
    .map_err(ProvingError::ProverExecution)?;

    let proof = Proof::from(proof_output.proof);
    let program_output = ProgramOutput::from(proof_output.output_preimage);

    Ok(ProverOutput { proof, program_output })
}
