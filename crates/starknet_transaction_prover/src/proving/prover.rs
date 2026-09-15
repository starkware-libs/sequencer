//! Prove a Cairo PIE and capture panics at the synchronous prover boundary.

use std::sync::Arc;

use cairo_vm::vm::runners::cairo_pie::CairoPie;
use privacy_prove::{privacy_recursive_prove, RecursiveProverPrecomputes};
use starknet_api::transaction::fields::Proof;
use starknet_proof_verifier::ProgramOutput;

use super::panic_capture::catch_panic_with_location;
use crate::errors::ProvingError;

#[derive(Debug, Clone)]
pub(crate) struct ProverOutput {
    pub proof: Proof,
    pub program_output: ProgramOutput,
}

pub(crate) async fn prove(
    cairo_pie: CairoPie,
    precomputes: Arc<RecursiveProverPrecomputes>,
) -> Result<ProverOutput, ProvingError> {
    let proof_output = tokio::task::spawn_blocking(move || {
        catch_panic_with_location(|| {
            privacy_recursive_prove(cairo_pie, precomputes).map_err(|error| error.to_string())
        })
        .map_err(|panic| ProvingError::ProverPanic {
            location: panic.location,
            message: panic.message,
        })?
        .map_err(ProvingError::ProverExecution)
    })
    .await
    .map_err(ProvingError::TaskJoin)??;

    Ok(ProverOutput {
        proof: Proof::from(proof_output.proof),
        program_output: ProgramOutput::from(proof_output.output_preimage),
    })
}
