use cairo_vm::vm::runners::cairo_pie::CairoPie;

pub struct StarknetOsOutput {}

pub struct StarknetOsRunnerOutput {
    pub os_output: StarknetOsOutput,
    pub cairo_pie: CairoPie,
}
