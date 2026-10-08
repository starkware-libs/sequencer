//! Cairo PIE proving using the privacy prover.
//!
//! Provides functionality to generate zero-knowledge proofs from Cairo PIE files.

use std::collections::BTreeMap;
use std::sync::Arc;

use cairo_vm::types::builtin_name::BuiltinName;
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
/// A PIE that uses builtins the prover does not support is rejected before proving starts.
pub(crate) async fn prove(
    cairo_pie: CairoPie,
    precomputes: Arc<RecursiveProverPrecomputes>,
) -> Result<ProverOutput, ProvingError> {
    let unsupported_builtins =
        unsupported_builtin_usage(&cairo_pie.execution_resources.builtin_instance_counter);
    if !unsupported_builtins.is_empty() {
        return Err(ProvingError::UnsupportedBuiltins { unsupported_builtins });
    }

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

/// The builtins that `privacy_recursive_prove` fails to prove, in report order. The privacy
/// bootloader cannot load a PIE that uses ecdsa, and each of the others enables a prover component
/// outside the recursive verifier's component set. The ec_op and keccak components are outside that
/// set too, but the bootloader verifies those builtins with Cairo code, so they are supported. The
/// small-prover tests in `prover_test.rs` pin this list against the upstream prover.
const UNSUPPORTED_BUILTINS: [BuiltinName; 4] =
    [BuiltinName::ecdsa, BuiltinName::range_check96, BuiltinName::add_mod, BuiltinName::mul_mod];

/// Returns each unsupported builtin that the execution used, with its instance count. A missing or
/// zero counter means unused.
pub(crate) fn unsupported_builtin_usage(
    builtin_instance_counter: &BTreeMap<BuiltinName, usize>,
) -> Vec<(BuiltinName, usize)> {
    UNSUPPORTED_BUILTINS
        .into_iter()
        .filter_map(|builtin_name| {
            let n_instances = *builtin_instance_counter.get(&builtin_name)?;
            (n_instances > 0).then_some((builtin_name, n_instances))
        })
        .collect()
}
