use std::collections::{HashMap, HashSet};

use blockifier::blockifier_versioned_constants::VersionedConstants;
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
use crate::test_utils::errors::{
    Cairo0EntryPointRunnerError,
    ExplicitArgError,
    ImplicitArgError,
    LoadReturnValueError,
};

pub type Cairo0EntryPointRunnerResult<T> = Result<T, Cairo0EntryPointRunnerError>;

#[cfg(test)]
#[path = "cairo_runner_test.rs"]
mod test;

/// An arg passed by value (i.e., a felt, tuple, named tuple or struct).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueArg {
    Single(Felt),
    Array(Vec<Felt>),
    Composed(Vec<EndpointArg>),
}

/// An arg passed as a pointer. i.e., a pointer to a felt, tuple, named tuple or struct, or a
/// pointer to a pointer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PointerArg {
    Array(Vec<Felt>),
    Composed(Vec<EndpointArg>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

    /// Returns the size of the space the arg occupies on the stack (not including referenced
    /// addresses).
    fn memory_length(&self) -> usize {
        match self {
            EndpointArg::Value(value_arg) => match value_arg {
                ValueArg::Single(_) => 1,
                ValueArg::Array(array) => array.len(),
                ValueArg::Composed(endpoint_args) => {
                    endpoint_args.iter().map(Self::memory_length).sum()
                }
            },
            EndpointArg::Pointer(_) => 1,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
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
    BuiltinName::from_str(arg_name.strip_suffix("_ptr")?)
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
    entrypoint: &str,
) -> Cairo0EntryPointRunnerResult<Program> {
    let mut program_dict: HashMap<String, Value> =
        serde_json::from_str(program_str).map_err(Cairo0EntryPointRunnerError::ProgramSerde)?;
    program_dict.insert(
        "builtins".to_string(),
        Value::from_iter(ordered_builtins.iter().map(|b| b.to_str())),
    );
    let program_str_with_builtins =
        serde_json::to_string(&program_dict).map_err(Cairo0EntryPointRunnerError::ProgramSerde)?;
    Ok(Program::from_bytes(program_str_with_builtins.as_bytes(), Some(entrypoint))?)
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

/// Loads an endpoint argument from the VM.
/// The end address should point to the address after the last felt of the argument.
/// The expected value is used to determine the structure of the argument (the actual values of
/// the expected argument are ignored).
/// Returns the address of the first felt of the argument.
fn load_endpoint_arg_from_end_address(
    expected_value: &EndpointArg,
    end_address: Relocatable,
    vm: &VirtualMachine,
) -> Result<(EndpointArg, Relocatable), LoadReturnValueError> {
    match expected_value {
        EndpointArg::Value(value_arg) => match value_arg {
            ValueArg::Single(_) => {
                let start_address = (end_address - 1)?;
                Ok((
                    EndpointArg::Value(ValueArg::Single(
                        vm.get_integer(start_address)?.into_owned(),
                    )),
                    start_address,
                ))
            }
            ValueArg::Array(array) => {
                let start_address = (end_address - array.len())?;
                let mut felt_array = vec![];
                for i in 0..array.len() {
                    felt_array.push(vm.get_integer((start_address + i)?)?.into_owned());
                }
                Ok((EndpointArg::Value(ValueArg::Array(felt_array)), start_address))
            }
            ValueArg::Composed(endpoint_args) => {
                let mut current_address = end_address;
                let mut endpoint_arg_array = vec![];
                for endpoint_arg in endpoint_args.iter().rev() {
                    let (value, arg_start_address) =
                        load_endpoint_arg_from_end_address(endpoint_arg, current_address, vm)?;
                    endpoint_arg_array.push(value);
                    current_address = arg_start_address;
                }
                endpoint_arg_array.reverse();
                Ok((EndpointArg::Value(ValueArg::Composed(endpoint_arg_array)), current_address))
            }
        },
        EndpointArg::Pointer(pointer_arg) => match pointer_arg {
            PointerArg::Array(array) => {
                let pointer_address = (end_address - 1)?;
                let array_start = vm.get_relocatable(pointer_address)?;
                let mut felt_array = vec![];
                for i in 0..array.len() {
                    felt_array.push(vm.get_integer((array_start + i)?)?.into_owned());
                }
                Ok((EndpointArg::Pointer(PointerArg::Array(felt_array)), pointer_address))
            }
            PointerArg::Composed(endpoint_args) => {
                let pointer_address = (end_address - 1)?;
                let mut current_address = vm.get_relocatable(pointer_address)?;
                let mut endpoint_arg_array = vec![];
                for expected_endpoint_arg in endpoint_args.iter() {
                    let arg_end_address =
                        (current_address + expected_endpoint_arg.memory_length())?;
                    let (value, arg_start_address) = load_endpoint_arg_from_end_address(
                        expected_endpoint_arg,
                        arg_end_address,
                        vm,
                    )?;
                    assert_eq!(
                        current_address, arg_start_address,
                        "The endpoint arg's start address {:?} does not match the returned start \
                         address {:?}",
                        current_address, arg_start_address
                    );
                    endpoint_arg_array.push(value);
                    current_address = arg_end_address;
                }
                Ok((
                    EndpointArg::Pointer(PointerArg::Composed(endpoint_arg_array)),
                    pointer_address,
                ))
            }
        },
    }
}

/// Loads the Explicit and implicit return values from the VM.
fn get_return_values(
    implicit_args: &[ImplicitArg],
    expected_return_values: &[EndpointArg],
    vm: &VirtualMachine,
) -> Result<(Vec<ImplicitArg>, Vec<EndpointArg>), LoadReturnValueError> {
    let mut arg_end_address = vm.get_ap(); // The address after the last return value.
    let mut explicit_return_values: Vec<EndpointArg> = vec![];
    for expected_return_value in expected_return_values.iter().rev() {
        let (value, start_address) =
            load_endpoint_arg_from_end_address(expected_return_value, arg_end_address, vm)?;
        explicit_return_values.push(value);
        arg_end_address = start_address;
    }

    let mut implicit_return_values: Vec<ImplicitArg> = vec![];
    for expected_return_value in implicit_args.iter().rev() {
        match expected_return_value {
            ImplicitArg::Builtin(builtin) => {
                implicit_return_values.push(ImplicitArg::Builtin(*builtin));
                arg_end_address = (arg_end_address - 1)?;
            }
            ImplicitArg::NonBuiltin(non_builtin_return_value) => {
                let (value, start_address) = load_endpoint_arg_from_end_address(
                    non_builtin_return_value,
                    arg_end_address,
                    vm,
                )?;
                implicit_return_values.push(ImplicitArg::NonBuiltin(value));
                arg_end_address = start_address;
            }
        }
    }
    implicit_return_values.reverse();
    explicit_return_values.reverse();
    Ok((implicit_return_values, explicit_return_values))
}

// TODO(Amos): Add logs to the runner.
// TODO(Amos): Return different errors in different stages of the runner, for easier debugging.
// e.g., `ReturnValueError`.
pub fn run_cairo_0_entry_point(
    program_str: &str,
    entrypoint: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_return_values: &[EndpointArg],
) -> Cairo0EntryPointRunnerResult<(Vec<ImplicitArg>, Vec<EndpointArg>)> {
    let ordered_builtins = get_ordered_builtins()?;
    let program = inject_builtins(program_str, &ordered_builtins, entrypoint)?;
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
    Ok(get_return_values(implicit_args, expected_explicit_return_values, &cairo_runner.vm)?)
}
