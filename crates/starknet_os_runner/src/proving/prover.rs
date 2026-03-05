//! Cairo PIE proving using the privacy prover.
//!
//! Provides functionality to generate zero-knowledge proofs from Cairo PIE files.

use apollo_transaction_converter::ProgramOutput;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use privacy_prove::privacy_prove;
use starknet_api::transaction::fields::Proof;
use tempfile::NamedTempFile;

use crate::errors::ProvingError;

/// Output from the prover containing the proof and associated program output.
#[derive(Debug, Clone)]
pub(crate) struct ProverOutput {
    /// The proof packed as u32s.
    pub proof: Proof,
    /// Raw program output from the bootloader (first element is number of tasks).
    pub program_output: ProgramOutput,
}

/// Proves a Cairo PIE using the privacy prover.
///
/// Writes the CairoPie to a temporary file, then calls `privacy_prove` on a blocking thread.
pub(crate) async fn prove(cairo_pie: CairoPie) -> Result<ProverOutput, ProvingError> {
    let temp_file = NamedTempFile::new().map_err(ProvingError::CreateTempFile)?;
    let pie_path = temp_file.path().to_path_buf();

    cairo_pie.write_zip_file(&pie_path, true).map_err(ProvingError::WriteCairoPie)?;

    let (proof_data, output_preimage) =
        tokio::task::spawn_blocking(move || privacy_prove(pie_path).map_err(|e| e.to_string()))
            .await
            .map_err(ProvingError::TaskJoin)?
            .map_err(ProvingError::ProverExecution)?;

    let proof = Proof::from(proof_data);
    let program_output = ProgramOutput::from(output_preimage);

    Ok(ProverOutput { proof, program_output })
}
