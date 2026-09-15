//! Cairo PIE proving using the privacy prover.
//!
//! Provides functionality to generate zero-knowledge proofs from Cairo PIE files.

use std::sync::Arc;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use privacy_circuit_verify_v2::consts::PRIVACY_TRANSACTION_COMPONENTS;
use privacy_prove::{privacy_recursive_prove, RecursiveProverPrecomputes};
use starknet_api::transaction::fields::Proof;
use starknet_proof_verifier::ProgramOutput;
use tokio::task::JoinError;

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
    let unsupported_builtins = used_unsupported_builtins(&cairo_pie);
    let proof_output = tokio::task::spawn_blocking(move || {
        privacy_recursive_prove(cairo_pie, precomputes).map_err(|e| e.to_string())
    })
    .await
    .map_err(|error| classify_prover_task_error(error, unsupported_builtins))?
    .map_err(ProvingError::ProverExecution)?;

    let proof = Proof::from(proof_output.proof);
    let program_output = ProgramOutput::from(proof_output.output_preimage);

    Ok(ProverOutput { proof, program_output })
}

pub(crate) fn used_unsupported_builtins(cairo_pie: &CairoPie) -> Vec<(BuiltinName, usize)> {
    [(BuiltinName::add_mod, "add_mod_builtin"), (BuiltinName::mul_mod, "mul_mod_builtin")]
        .into_iter()
        .filter(|(_, component)| !PRIVACY_TRANSACTION_COMPONENTS.contains(component))
        .filter_map(|(builtin, _)| {
            let count = *cairo_pie.execution_resources.builtin_instance_counter.get(&builtin)?;
            (count > 0).then_some((builtin, count))
        })
        .collect()
}

pub(super) fn classify_prover_task_error(
    error: JoinError,
    unsupported_builtins: Vec<(BuiltinName, usize)>,
) -> ProvingError {
    // TODO: Have privacy-prove return a typed unsupported-component error and match it here.
    // Builtin usage alone does not establish the cause of a prover panic.
    if error.is_panic() && !unsupported_builtins.is_empty() {
        ProvingError::UnsupportedBuiltins { unsupported_builtins, reason: error.to_string() }
    } else {
        ProvingError::TaskJoin(error)
    }
}
