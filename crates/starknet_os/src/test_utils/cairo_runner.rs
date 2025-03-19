use std::collections::{HashMap, HashSet};

use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::execution::call_info::Retdata;
use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::utils::is_subsequence;
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};
use cairo_vm::vm::vm_core::VirtualMachine;
use serde_json::Value;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::test_utils::errors::{Cairo0EntryPointRunnerError, ExplicitArgError, ImplicitArgError};

pub type Cairo0EntryPointRunnerResult<T> = Result<T, Cairo0EntryPointRunnerError>;

#[cfg(test)]
#[path = "cairo_runner_test.rs"]
mod test;

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

#[derive(Debug, Clone)]
pub enum ImplicitArg {
    Builtin(BuiltinName),
    NonBuiltin(EndpointArg),
}

/// Performs basic validations on the cairo arg. Assumes the arg is not a builtin.
/// A successful result from this function does NOT guarantee that the arguments are valid.
fn perform_basic_validations_on_endpoint_arg(
    index: usize,
    expected_arg: &Member,
    actual_arg: &EndpointArg,
) -> Cairo0EntryPointRunnerResult<()> {
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
) -> Cairo0EntryPointRunnerResult<()> {
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
        perform_basic_validations_on_endpoint_arg(index, expected_arg, actual_arg)?;
    }
    Ok(())
}

fn get_builtin_or_non(arg_name: &str) -> Option<BuiltinName> {
    if let Some(name_without_suffix) = arg_name.strip_suffix("_ptr") {
        BuiltinName::from_str(name_without_suffix)
    } else {
        None
    }
}

/// Performs basic validations on the implicit arguments. A successful result from this function
/// does NOT guarantee that the arguments are valid.
fn perform_basic_validations_on_implicit_args(
    implicit_args: &[ImplicitArg],
    program: &Program,
    entrypoint: &str,
    ordered_builtins: &[BuiltinName],
) -> Cairo0EntryPointRunnerResult<()> {
    let mut expected_implicit_args: Vec<(String, Member)> = program
        .get_identifier(&format!("__main__.{}.ImplicitArgs", entrypoint))
        .unwrap_or_else(|| {
            panic!("Found no implicit args identifier for entrypoint {}.", entrypoint)
        })
        .members
        .as_ref()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();

    expected_implicit_args.sort_by(|a, b| a.1.offset.cmp(&b.1.offset));
    if expected_implicit_args.len() != implicit_args.len() {
        Err(ImplicitArgError::WrongNumberOfArgs {
            expected: expected_implicit_args.clone(),
            actual: implicit_args.to_vec(),
        })?;
    }
    let mut actual_builtins: Vec<BuiltinName> = vec![];
    for (index, actual_arg) in implicit_args.iter().enumerate() {
        let (expected_arg_name, expected_arg) = &expected_implicit_args[index];
        let expected_builtin_or_none = get_builtin_or_non(expected_arg_name);
        let actual_builtin_or_none = match actual_arg {
            ImplicitArg::Builtin(builtin) => Some(*builtin),
            ImplicitArg::NonBuiltin(_) => None,
        };
        if expected_builtin_or_none != actual_builtin_or_none {
            Err(ImplicitArgError::Mismatch {
                index,
                expected: expected_arg.clone(),
                actual: actual_arg.clone(),
            })?;
        }
        match actual_arg {
            ImplicitArg::Builtin(builtin) => {
                actual_builtins.push(*builtin);
                continue;
            }
            ImplicitArg::NonBuiltin(endpoint_arg) => {
                perform_basic_validations_on_endpoint_arg(index, expected_arg, endpoint_arg)?;
            }
        }
    }
    if !is_subsequence(&actual_builtins, ordered_builtins) {
        Err(ImplicitArgError::WrongBuiltinOrder {
            correct_order: ordered_builtins.to_vec(),
            actual_order: actual_builtins,
        })?;
    }
    Ok(())
}

// This is a hack to add the entrypoint's builtins:
// Create a program with all the builtins, and only use the relevant builtins for the
// entrypoint.
// TODO(Amos): Add builtins properly once the VM allows loading an entrypoint's builtins.
// In addition, pass program as struct and add hint processor as param.
fn inject_builtins(
    program_str: &str,
    ordered_builtins: &[BuiltinName],
) -> Cairo0EntryPointRunnerResult<Program> {
    let mut program_dict: HashMap<String, Value> =
        serde_json::from_str(program_str).map_err(Cairo0EntryPointRunnerError::ProgramSerde)?;
    program_dict.insert(
        "builtins".to_string(),
        Value::from_iter(ordered_builtins.iter().map(|b| b.to_str())),
    );
    let program_str_with_builtins =
        serde_json::to_string(&program_dict).map_err(Cairo0EntryPointRunnerError::ProgramSerde)?;
    Ok(Program::from_bytes(program_str_with_builtins.as_bytes(), None)?)
}

fn convert_implicit_args_to_cairo_args(
    implicit_args: &[ImplicitArg],
    vm: &VirtualMachine,
    ordered_builtins: &[BuiltinName],
) -> Vec<CairoArg> {
    let all_builtins_initial_stacks: Vec<Vec<MaybeRelocatable>> = vm
        .get_builtin_runners()
        .iter()
        .map(|builtin_runner| builtin_runner.initial_stack())
        .collect();
    let all_builtin_map: HashMap<_, _> =
        ordered_builtins.iter().zip(all_builtins_initial_stacks).collect();
    implicit_args
        .iter()
        .flat_map(|arg| match arg {
            ImplicitArg::Builtin(builtin) => vec![CairoArg::from(all_builtin_map[builtin].clone())],
            ImplicitArg::NonBuiltin(endpoint_arg) => EndpointArg::to_cairo_arg_vec(endpoint_arg),
        })
        .collect()
}

fn get_ordered_builtins() -> Cairo0EntryPointRunnerResult<Vec<BuiltinName>> {
    let ordered_builtins = vec![
        BuiltinName::output,
        BuiltinName::pedersen,
        BuiltinName::range_check,
        BuiltinName::ecdsa,
        BuiltinName::bitwise,
        BuiltinName::ec_op,
        BuiltinName::keccak,
        BuiltinName::poseidon,
        BuiltinName::range_check96,
        BuiltinName::add_mod,
        BuiltinName::mul_mod,
    ];
    let actual_builtins = VersionedConstants::latest_constants()
        .vm_resource_fee_cost()
        .builtins
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    if ordered_builtins.iter().cloned().collect::<HashSet<_>>() != actual_builtins {
        Err(Cairo0EntryPointRunnerError::BuiltinMismatch {
            cairo_runner_builtins: ordered_builtins.clone(),
            actual_builtins,
        })?;
    }
    Ok(ordered_builtins)
}

pub fn run_cairo_0_entry_point(
    program_str: &str,
    entrypoint: &str,
    n_expected_return_values: usize,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
) -> Cairo0EntryPointRunnerResult<Retdata> {
    let ordered_builtins = get_ordered_builtins()?;
    let program = inject_builtins(program_str, &ordered_builtins)?;
    let (state_reader, os_input) = (None, None);
    let mut hint_processor =
        SnosHintProcessor::new_for_testing(state_reader, os_input, Some(program.clone()));

    // TODO(Amos): Perform complete validations.
    perform_basic_validations_on_explicit_args(explicit_args, &program, entrypoint)?;
    perform_basic_validations_on_implicit_args(
        implicit_args,
        &program,
        entrypoint,
        &ordered_builtins,
    )?;

    let proof_mode = false;
    let trace_enabled = true;
    let mut cairo_runner =
        CairoRunner::new(&program, LayoutName::all_cairo, proof_mode, trace_enabled).unwrap();

    let allow_missing_builtins = false;
    cairo_runner.initialize_builtins(allow_missing_builtins).unwrap();
    let program_base: Option<Relocatable> = None;
    cairo_runner.initialize_segments(program_base);

    let explicit_cairo_args: Vec<CairoArg> =
        explicit_args.iter().flat_map(EndpointArg::to_cairo_arg_vec).collect();

    let implicit_cairo_args =
        convert_implicit_args_to_cairo_args(implicit_args, &cairo_runner.vm, &ordered_builtins);

    let entrypoint_args: Vec<&CairoArg> =
        implicit_cairo_args.iter().chain(explicit_cairo_args.iter()).collect();

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

    // TODO(Amos): Return implicit arguments, once the runner supports returning non-felt types.
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
