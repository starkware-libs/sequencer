use blockifier::execution::call_info::Retdata;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessor;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};
use cairo_vm::Felt252;

pub fn run_cairo_0_entry_point(
    program: &Program,
    entrypoint: &str,
    n_expected_return_values: usize,
    args: &[MaybeRelocatable],
    mut hint_processor: impl HintProcessor,
) -> Result<Retdata, CairoRunError> {
    let proof_mode = false;
    let trace_enabled = true;
    let mut cairo_runner =
        CairoRunner::new(program, LayoutName::all_cairo, proof_mode, trace_enabled).unwrap();
    cairo_runner.initialize_builtins(false).unwrap();
    cairo_runner.initialize_segments(None);

    let mut entrypoint_args: Vec<CairoArg> = vec![
        MaybeRelocatable::from(Felt252::from(2_i128)).into(), // this is the entry point selector
        // this would be the output_ptr for example if our cairo function uses it
        MaybeRelocatable::from((2, 0)).into(),
    ];
    entrypoint_args.extend(args.iter().map(|arg| CairoArg::from(arg.clone())));
    let entrypoint_args: Vec<&CairoArg> = entrypoint_args.iter().collect();
    let verify_secure = true;
    let program_segment_size: Option<usize> = None;
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
            .iter()
            .map(|m| match m {
                MaybeRelocatable::Int(i) => *i,
                MaybeRelocatable::RelocatableValue(relocatable) => cairo_runner
                    .vm
                    .get_integer(*relocatable)
                    .unwrap_or_else(|err| {
                        panic!(
                            "Could not convert relocatable {:?} to integer. error: {:?}",
                            relocatable, err
                        )
                    })
                    .into_owned(),
            })
            .collect(),
    ))
}
