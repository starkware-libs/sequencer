use cairo_vm::vm::runners::cairo_pie::CairoPie;
use starknet_types_core::felt::Felt;

/// The output of the virtual OS runner.
#[derive(Debug)]
pub struct VirtualOsRunnerOutput {
    /// The raw virtual OS output.
    pub raw_output: Vec<Felt>,
    /// The Cairo PIE (Program Independent Execution) artifact.
    pub cairo_pie: CairoPie,
}
