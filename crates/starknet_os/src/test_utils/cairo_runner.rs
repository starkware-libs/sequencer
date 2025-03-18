use blockifier::execution::call_info::Retdata;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessor;
use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};

use crate::test_utils::errors::{Cairo0EntryPointRunnerError, ExplicitArgError};

/// Performs basic validations on the cairo arg. Assumes the arg is not a builtin.
/// A successful result from this function does NOT guarantee that the arguments are valid.
fn perform_basic_validations_on_cairo_arg(
    index: usize,
    expected_arg: &Member,
    actual_arg: &CairoArg,
) -> Result<(), Cairo0EntryPointRunnerError> {
    if matches!(actual_arg, CairoArg::Single(MaybeRelocatable::RelocatableValue(_))) {
        Err(ExplicitArgError::SingleRelocatableParam { index, actual_arg: actual_arg.clone() })?
    }
    let actual_arg_is_felt = matches!(actual_arg, CairoArg::Single(MaybeRelocatable::Int(_)));
    let actual_arg_is_single = matches!(actual_arg, CairoArg::Single(_));

    let expected_arg_is_pointer = expected_arg.cairo_type.ends_with("*");
    let expected_arg_is_felt = expected_arg.cairo_type == "felt";

    if expected_arg_is_felt != actual_arg_is_felt || expected_arg_is_pointer == actual_arg_is_single
    {
        Err(ExplicitArgError::Mismatch {
            index,
            expected: expected_arg.clone(),
            actual: actual_arg.clone(),
        })?;
    };
    // expected arg is tuple / named tuple / struct.
    if !expected_arg_is_felt && !expected_arg_is_pointer {
        // TODO(Amos): Load tuple / named tuple / struct parameters to stack and remove this error.
        Err(ExplicitArgError::UnsupportedArgType { index, expected_arg: expected_arg.clone() })?;
    };
    Ok(())
}

/// Performs basic validations on the explicit arguments. A successful result from this function
/// does NOT guarantee that the arguments are valid.
fn perform_basic_validations_on_explicit_args(
    explicit_args: &[CairoArg],
    program: &Program,
    entrypoint: &str,
) -> Result<(), Cairo0EntryPointRunnerError> {
    let mut expected_explicit_args: Vec<Member> = program
        .get_identifier(&format!("__main__.{}.Args", entrypoint))
        .unwrap_or_else(|| {
            panic!("Found no explicit args identifier for entrypoint {}.", entrypoint)
        })
        .members
        .as_ref()
        .unwrap()
        .values()
        .cloned()
        .collect();

    if expected_explicit_args.len() != explicit_args.len() {
        Err(ExplicitArgError::WrongNumberOfArgs {
            expected: expected_explicit_args.to_vec(),
            actual: explicit_args.to_vec(),
        })?;
    }

    expected_explicit_args.sort_by(|a, b| a.offset.cmp(&b.offset));
    for (index, actual_arg) in explicit_args.iter().enumerate() {
        let expected_arg = expected_explicit_args.get(index).unwrap();
        perform_basic_validations_on_cairo_arg(index, expected_arg, actual_arg)?;
    }
    Ok(())
}

pub fn run_cairo_0_entry_point(
    program: &Program,
    entrypoint: &str,
    n_expected_return_values: usize,
    explicit_args: &[CairoArg],
    mut hint_processor: impl HintProcessor,
) -> Result<Retdata, Cairo0EntryPointRunnerError> {
    // TODO(Amos): Perform complete validations.
    perform_basic_validations_on_explicit_args(explicit_args, program, entrypoint)?;
    let proof_mode = false;
    let trace_enabled = true;
    let mut cairo_runner =
        CairoRunner::new(program, LayoutName::all_cairo, proof_mode, trace_enabled).unwrap();

    let allow_missing_builtins = false;
    cairo_runner.initialize_builtins(allow_missing_builtins).unwrap();
    let program_base: Option<Relocatable> = None;
    cairo_runner.initialize_segments(program_base);

    let entrypoint_args: Vec<&CairoArg> = explicit_args.iter().collect();
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
                    .unwrap_or_else(|| panic!("Could not convert return data {:?} to integer.", m))
            })
            .collect(),
    ))
}
