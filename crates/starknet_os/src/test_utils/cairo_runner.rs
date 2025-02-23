use cairo_vm::hint_processor::hint_processor_definition::HintProcessor;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};
use cairo_vm::Felt252;

pub fn run_cairo_0_entry_point(
    program_content: &[u8],
    entrypoint: &str,
    args: &[MaybeRelocatable],
    expected_retdata: &[Felt252],
    mut hint_processor: impl HintProcessor,
) -> Result<(), CairoRunError> {
    let program = Program::from_bytes(program_content, None).unwrap();
    let mut cairo_runner = CairoRunner::new(&program, LayoutName::all_cairo, false, true).unwrap();
    cairo_runner.initialize_function_runner()?;

    // Implicit args.
    let mut entrypoint_args: Vec<CairoArg> = vec![
        MaybeRelocatable::from(Felt252::from(2_i128)).into(), // this is the entry point selector
        // this would be the output_ptr for example if our cairo function uses it
        MaybeRelocatable::from((2, 0)).into(),
    ];
    // Explicit args.
    let calldata_start = cairo_runner.vm.add_memory_segment();
    let calldata_end = cairo_runner.vm.load_data(calldata_start, args).unwrap();
    entrypoint_args.extend([
        MaybeRelocatable::from(calldata_start).into(),
        MaybeRelocatable::from(calldata_end).into(),
    ]);
    let entrypoint_args: Vec<&CairoArg> = entrypoint_args.iter().collect();

    cairo_runner.run_from_entrypoint(
        program
            .get_identifier(&format!("__main__.{}", entrypoint))
            .unwrap_or_else(|| panic!("entrypoint {} not found.", entrypoint))
            .pc
            .unwrap(),
        &entrypoint_args,
        true,
        None,
        &mut hint_processor,
    )?;

    // Check return values
    let return_values = cairo_runner.vm.get_return_values(5).unwrap();
    let retdata_start = return_values[3].get_relocatable().unwrap();
    let retdata_end = return_values[4].get_relocatable().unwrap();
    let retdata: Vec<Felt252> = cairo_runner
        .vm
        .get_integer_range(retdata_start, (retdata_end - retdata_start).unwrap())
        .unwrap()
        .iter()
        .map(|c| c.clone().into_owned())
        .collect();
    assert_eq!(expected_retdata, &retdata);
    Ok(())
}
