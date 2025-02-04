use cairo_vm::vm::runners::cairo_pie::CairoPie;

// TODO(Dori): Add fields.
pub struct StarknetOsOutput {}

pub struct StarknetOsRunnerOutput {
    pub os_output: StarknetOsOutput,
    pub cairo_pie: CairoPie,
}
