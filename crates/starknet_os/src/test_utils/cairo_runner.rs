use blockifier::execution::call_info::Retdata;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessor;
use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};
use starknet_types_core::felt::Felt;

use crate::test_utils::errors::{Cairo0EntryPointRunnerError, ExplicitArgError};

#[cfg(test)]
#[path = "cairo_runner_test.rs"]
mod test;

#[derive(Clone, Debug)]
pub enum EndpointArg {
    Value(ValueArg),
    Pointer(PointerArg),
}

impl From<i32> for EndpointArg {
    fn from(value: i32) -> Self {
        Self::Value(ValueArg::Single(value.into()))
    }
}

impl EndpointArg {
    /// Converts an endpoint arg into a vector of cairo args.
    /// The cairo VM loads struct / tuple / named tuple parameters by adding each of their fields
    /// to the stack. This is why a single endpoint arg can be converted into multiple cairo args -
    /// an arg of type Struct {a: felt, b: felt} will be converted into a vector of two cairo args
    /// of type felt.
    fn to_cairo_arg_vec(endpoint_arg: &EndpointArg) -> Vec<CairoArg> {
        match endpoint_arg {
            EndpointArg::Value(value_arg) => match value_arg {
                ValueArg::Single(felt) => {
                    vec![CairoArg::Single(MaybeRelocatable::Int(*felt))]
                }
                ValueArg::Array(felts) => felts
                    .iter()
                    .map(|felt| CairoArg::Single(MaybeRelocatable::Int(*felt)))
                    .collect(),
                ValueArg::Composed(endpoint_args) => {
                    endpoint_args.iter().flat_map(Self::to_cairo_arg_vec).collect()
                }
            },
            EndpointArg::Pointer(pointer_arg) => match pointer_arg {
                PointerArg::Array(felts) => vec![CairoArg::Array(
                    felts.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
                )],
                PointerArg::Composed(endpoint_args) => vec![CairoArg::Composed(
                    endpoint_args.iter().flat_map(Self::to_cairo_arg_vec).collect(),
                )],
            },
        }
    }
}

/// An arg passed by value (i.e., a felt, tuple, named tuple or struct).
#[derive(Clone, Debug)]
pub enum ValueArg {
    Single(Felt),
    Array(Vec<Felt>),
    Composed(Vec<EndpointArg>),
}

/// An arg passed as a pointer. i.e., a pointer to a felt, tuple, named tuple or struct, or a
/// pointer to a pointer.
#[derive(Clone, Debug)]
pub enum PointerArg {
    Array(Vec<Felt>),
    Composed(Vec<EndpointArg>),
}

/// Performs basic validations on the cairo arg. Assumes the arg is not a builtin.
/// A successful result from this function does NOT guarantee that the arguments are valid.
fn perform_basic_validations_on_cairo_arg(
    index: usize,
    expected_arg: &Member,
    actual_arg: &EndpointArg,
) -> Result<(), Cairo0EntryPointRunnerError> {
    let actual_arg_is_felt = matches!(actual_arg, EndpointArg::Value(ValueArg::Single(_)));
    let actual_arg_is_pointer = matches!(actual_arg, EndpointArg::Pointer(_));
    let actual_arg_is_struct_or_tuple = !actual_arg_is_felt && !actual_arg_is_pointer;

    let expected_arg_is_pointer = expected_arg.cairo_type.ends_with("*");
    let expected_arg_is_felt = expected_arg.cairo_type == "felt";
    let expected_arg_is_struct_or_tuple = !expected_arg_is_felt && !expected_arg_is_pointer;

    if expected_arg_is_felt != actual_arg_is_felt
        || expected_arg_is_pointer != actual_arg_is_pointer
        || expected_arg_is_struct_or_tuple != actual_arg_is_struct_or_tuple
    {
        Err(ExplicitArgError::Mismatch {
            index,
            expected: expected_arg.clone(),
            actual: actual_arg.clone(),
        })?;
    };
    Ok(())
}

/// Performs basic validations on the explicit arguments. A successful result from this function
/// does NOT guarantee that the arguments are valid.
fn perform_basic_validations_on_explicit_args(
    explicit_args: &[EndpointArg],
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
    explicit_args: &[EndpointArg],
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

    let entrypoint_args: Vec<CairoArg> =
        explicit_args.iter().flat_map(EndpointArg::to_cairo_arg_vec).collect();
    let entrypoint_args: Vec<&CairoArg> = entrypoint_args.iter().collect();
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
