use blockifier::execution::call_info::Retdata;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessor;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};

pub fn run_cairo_0_entry_point(
    program: &Program,
    entrypoint: &str,
    n_expected_return_values: usize,
    args: &[CairoArg],
    mut hint_processor: impl HintProcessor,
) -> Result<Retdata, CairoRunError> {
    let proof_mode = false;
    let trace_enabled = true;
    let mut cairo_runner =
        CairoRunner::new(program, LayoutName::all_cairo, proof_mode, trace_enabled).unwrap();

    let allow_missing_builtins = false;
    cairo_runner.initialize_builtins(allow_missing_builtins).unwrap();
    let program_base: Option<Relocatable> = None;
    cairo_runner.initialize_segments(program_base);

    let entrypoint_args: Vec<&CairoArg> = args.iter().collect();
    let verify_secure = true;
    let program_segment_size: Option<usize> = None;
    // TODO(Amos): Pass implicit args to the cairo runner.
    cairo_runner.run_from_entrypoint(
        program
            .get_identifier(&format!("__main__.{}", entrypoint))
            .unwrap_or_else(|| panic!("entrypoint {} not found.", entrypoint))
            .pc
            .unwrap(),
        &entrypoint_args,
        verify_secure,
        program_segment_size,
        &mut hint_processor,
    )?;

    // Check return values
    let return_values = cairo_runner.vm.get_return_values(n_expected_return_values).unwrap();
    Ok(Retdata(
        return_values
            .into_iter()
            .map(|m| {
                // TODO(Amos): Support returning types other than felts.
                m.get_int()
                    .unwrap_or_else(|| panic!("Could not convert return data {} to integer.", m))
            })
            .collect(),
    ))
}
