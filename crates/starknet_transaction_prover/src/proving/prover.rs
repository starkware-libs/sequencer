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

/// Proves a Cairo PIE and returns the proof and proof facts.
///
/// This is a standalone proving function that initializes its own precomputes.
/// Intended for use in fixture generation tests where no [`VirtualSnosProver`] is available.
///
/// [`VirtualSnosProver`]: crate::proving::virtual_snos_prover::VirtualSnosProver
pub async fn prove_cairo_pie_standalone(
    cairo_pie: CairoPie,
) -> Result<(Proof, starknet_api::transaction::fields::ProofFacts), String> {
    use privacy_prove::{prepare_recursive_prover_precomputes, privacy_recursive_prove};
    use starknet_api::transaction::fields::VIRTUAL_SNOS;

    // The stwo prover uses rayon for parallelism. Build a dedicated thread pool with a large
    // per-worker stack to prevent overflow in worker threads during recursive proving.
    tokio::task::spawn_blocking(move || {
        let pool = rayon::ThreadPoolBuilder::new()
            .stack_size(128 * 1024 * 1024)
            .build()
            .map_err(|e| format!("pool: {e}"))?;
        pool.install(|| -> Result<_, String> {
            // prepare_recursive_prover_precomputes returns Arc<RecursiveProverPrecomputes>.
            let precomputes =
                prepare_recursive_prover_precomputes().map_err(|e| format!("precomputes: {e}"))?;
            let proof_output = privacy_recursive_prove(cairo_pie, precomputes)
                .map_err(|e| format!("prove: {e}"))?;
            let proof_bytes: Vec<u8> =
                proof_output.proof.iter().flat_map(|n| n.to_be_bytes()).collect();
            let proof = Proof::from(proof_bytes);
            let program_output = ProgramOutput::from(proof_output.output_preimage);
            let proof_facts = program_output
                .try_into_proof_facts(VIRTUAL_SNOS)
                .map_err(|e| format!("proof_facts: {e:?}"))?;
            Ok((proof, proof_facts))
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}
