use std::any::Any;
use std::collections::{HashMap, HashSet};

use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::utils::is_subsequence;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::runners::builtin_runner::BuiltinRunner;
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};
use cairo_vm::vm::vm_core::VirtualMachine;
use log::{debug, info};
use serde_json::Value;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::io::os_input::OsBlockInput;
use crate::test_utils::errors::{
    BuiltinMismatchError,
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
    Single(MaybeRelocatable),
    Array(Vec<MaybeRelocatable>),
    Composed(Vec<EndpointArg>),
}

/// An arg passed as a pointer. i.e., a pointer to a felt, tuple, named tuple or struct, or a
/// pointer to a pointer.
// TODO(Nimrod): Extend this to be able to return an entire segment without knowing the length in
// advance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PointerArg {
    Array(Vec<MaybeRelocatable>),
    Composed(Vec<EndpointArg>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EndpointArg {
    Value(ValueArg),
    Pointer(PointerArg),
}

impl From<i32> for EndpointArg {
    fn from(value: i32) -> Self {
        Self::from(Felt::from(value))
    }
}

impl From<usize> for EndpointArg {
    fn from(value: usize) -> Self {
        Self::from(Felt::from(value))
    }
}

impl From<u128> for EndpointArg {
    fn from(value: u128) -> Self {
        Self::from(Felt::from(value))
    }
}

impl From<Felt> for EndpointArg {
    fn from(value: Felt) -> Self {
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
                ValueArg::Single(val) => {
                    vec![CairoArg::Single(val.clone())]
                }
                ValueArg::Array(arr) => {
                    arr.iter().map(|val| CairoArg::Single(val.clone())).collect()
                }
                ValueArg::Composed(endpoint_args) => {
                    endpoint_args.iter().flat_map(Self::to_cairo_arg_vec).collect()
                }
            },
            EndpointArg::Pointer(pointer_arg) => match pointer_arg {
                PointerArg::Array(felts) => {
                    vec![CairoArg::Array(felts.to_vec())]
                }
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
            Self::Value(value_arg) => match value_arg {
                ValueArg::Single(_) => 1,
                ValueArg::Array(array) => array.len(),
                ValueArg::Composed(endpoint_args) => {
                    endpoint_args.iter().map(Self::memory_length).sum()
                }
            },
            Self::Pointer(_) => 1,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ImplicitArg {
    Builtin(BuiltinName),
    NonBuiltin(EndpointArg),
}

impl ImplicitArg {
    /// Returns the size of the space the arg occupies on the stack (not including referenced
    /// addresses).
    fn memory_length(&self) -> usize {
        match self {
            Self::Builtin(_) => 1,
            Self::NonBuiltin(endpoint_arg) => endpoint_arg.memory_length(),
        }
    }
}

pub struct EntryPointRunnerConfig {
    pub trace_enabled: bool,
    pub verify_secure: bool,
    pub layout: LayoutName,
    pub proof_mode: bool,
    // If true, the entrypoint will be prefixed with __main__.
    pub add_main_prefix_to_entrypoint: bool,
}

impl Default for EntryPointRunnerConfig {
    fn default() -> Self {
        Self {
            trace_enabled: false,
            verify_secure: true,
            layout: LayoutName::plain,
            proof_mode: false,
            add_main_prefix_to_entrypoint: true,
        }
    }
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
        .get_identifier(&format!("{entrypoint}.Args"))
        .unwrap_or_else(|| panic!("Found no explicit args identifier for entrypoint {entrypoint}."))
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
) -> Cairo0EntryPointRunnerResult<()> {
    let mut expected_implicit_args: Vec<(String, Member)> = program
        .get_identifier(&format!("{entrypoint}.ImplicitArgs"))
        .unwrap_or_else(|| panic!("Found no implicit args identifier for entrypoint {entrypoint}."))
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
    Ok(())
}

fn extract_builtins_from_implicit_args(
    implicit_args: &[ImplicitArg],
) -> Cairo0EntryPointRunnerResult<Vec<BuiltinName>> {
    let all_builtins_ordered = get_all_builtins_ordered()?;
    let program_builtins: Vec<BuiltinName> = implicit_args
        .iter()
        .filter_map(
            |arg| {
                if let ImplicitArg::Builtin(builtin) = arg { Some(*builtin) } else { None }
            },
        )
        .collect();
    if !is_subsequence(&program_builtins, &all_builtins_ordered) {
        Err(ImplicitArgError::WrongBuiltinOrder {
            correct_order: all_builtins_ordered.to_vec(),
            actual_order: program_builtins.clone(),
        })?;
    }
    Ok(program_builtins)
}

// This is a hack to inject the entrypoint's builtins into the program.
// TODO(Amos): Add builtins properly once the VM allows loading an entrypoint's builtins.
// In addition, pass program as struct and add hint processor as param.
fn inject_builtins(
    program_bytes: &[u8],
    implicit_args: &[ImplicitArg],
) -> Cairo0EntryPointRunnerResult<Program> {
    let program_builtins = extract_builtins_from_implicit_args(implicit_args)?;
    let program_str = std::str::from_utf8(program_bytes).unwrap();
    let mut program_dict: HashMap<String, Value> =
        serde_json::from_str(program_str).map_err(Cairo0EntryPointRunnerError::ProgramSerde)?;
    program_dict.insert(
        "builtins".to_string(),
        Value::from_iter(program_builtins.iter().map(|b| b.to_str())),
    );
    let program_str_with_builtins =
        serde_json::to_string(&program_dict).map_err(Cairo0EntryPointRunnerError::ProgramSerde)?;
    Ok(Program::from_bytes(program_str_with_builtins.as_bytes(), None)?)
}

fn convert_implicit_args_to_cairo_args(
    implicit_args: &[ImplicitArg],
    vm: &VirtualMachine,
) -> Vec<CairoArg> {
    let mut initial_stacks_iterator = vm.get_builtin_runners().iter().map(|builtin_runner| {
        let initial_stack_vec = builtin_runner.initial_stack();
        assert_eq!(
            initial_stack_vec.len(),
            1,
            "Expected initial stack to be of length 1, but got {}",
            initial_stack_vec.len()
        );
        let initial_stack: CairoArg = initial_stack_vec.first().unwrap().clone().into();
        vec![initial_stack]
    });
    implicit_args
        .iter()
        .flat_map(|arg| match arg {
            ImplicitArg::Builtin(builtin) => initial_stacks_iterator
                .next()
                .unwrap_or_else(|| panic!("No builtin runner found for builtin {builtin}.")),
            ImplicitArg::NonBuiltin(endpoint_arg) => EndpointArg::to_cairo_arg_vec(endpoint_arg),
        })
        .collect()
}

fn get_all_builtins_ordered() -> Result<Vec<BuiltinName>, BuiltinMismatchError> {
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
        Err(BuiltinMismatchError {
            cairo_runner_builtins: ordered_builtins.clone(),
            actual_builtins,
        })?;
    }
    Ok(ordered_builtins)
}

/// A helper function for `load_endpoint_arg_from_address`.
/// Loads a sequence of maybe relocatables from memory and returns it.
fn load_sequence_of_maybe_relocatables(
    length: usize,
    base_address: Relocatable,
    vm: &VirtualMachine,
) -> Result<Vec<MaybeRelocatable>, LoadReturnValueError> {
    let mut array = vec![];
    for i in 0..length {
        let current_address = (base_address + i)?;
        array.push(
            vm.get_maybe(&current_address)
                .ok_or(MemoryError::UnknownMemoryCell(current_address.into()))?,
        );
    }
    Ok(array)
}

/// A helper function for `load_endpoint_arg_from_address`.
/// Loads a sequence of endpoint args from memory and returns it, together with the first address
/// after the sequence.
fn load_sequence_of_endpoint_args(
    sequence: &[EndpointArg],
    address: Relocatable,
    vm: &VirtualMachine,
) -> Result<(Vec<EndpointArg>, Relocatable), LoadReturnValueError> {
    let mut current_address = address;
    let mut endpoint_args = vec![];
    for endpoint_arg in sequence.iter() {
        let (value, next_address) =
            load_endpoint_arg_from_address(endpoint_arg, current_address, vm)?;
        endpoint_args.push(value);
        current_address = next_address;
    }
    Ok((endpoint_args, current_address))
}

/// Loads a value from the VM and returns it, together with the address after said value.
/// Note - the values inside `value_structure` are ignored.
fn load_endpoint_arg_from_address(
    value_structure: &EndpointArg,
    address: Relocatable,
    vm: &VirtualMachine,
) -> Result<(EndpointArg, Relocatable), LoadReturnValueError> {
    let value_size = value_structure.memory_length();
    match value_structure {
        EndpointArg::Value(value_arg) => match value_arg {
            ValueArg::Single(_) => Ok((
                EndpointArg::Value(ValueArg::Single(
                    vm.get_maybe(&address).ok_or(MemoryError::UnknownMemoryCell(address.into()))?,
                )),
                (address + value_size)?,
            )),
            ValueArg::Array(array) => {
                let array = load_sequence_of_maybe_relocatables(array.len(), address, vm)?;
                Ok((EndpointArg::Value(ValueArg::Array(array)), (address + value_size)?))
            }
            ValueArg::Composed(endpoint_args) => {
                let (endpoint_arg_array, next_address) =
                    load_sequence_of_endpoint_args(endpoint_args, address, vm)?;
                Ok((EndpointArg::Value(ValueArg::Composed(endpoint_arg_array)), next_address))
            }
        },
        EndpointArg::Pointer(pointer_arg) => match pointer_arg {
            PointerArg::Array(array) => {
                let array_pointer = vm.get_relocatable(address)?;
                let array = load_sequence_of_maybe_relocatables(array.len(), array_pointer, vm)?
                    .into_iter()
                    .collect();
                Ok((EndpointArg::Pointer(PointerArg::Array(array)), (address + value_size)?))
            }
            PointerArg::Composed(endpoint_args) => {
                let (endpoint_arg_array, _) = load_sequence_of_endpoint_args(
                    endpoint_args,
                    vm.get_relocatable(address)?,
                    vm,
                )?;
                Ok((
                    EndpointArg::Pointer(PointerArg::Composed(endpoint_arg_array)),
                    (address + value_size)?,
                ))
            }
        },
    }
}

/// Push the number of used instances of a builtin to the implicit return values.
fn push_n_used_instances(
    builtin_runner: &BuiltinRunner,
    implicit_return_values: &mut Vec<EndpointArg>,
    vm: &VirtualMachine,
) -> Result<(), LoadReturnValueError> {
    let n_used_instances = builtin_runner.get_used_instances(&vm.segments)?;
    implicit_return_values
        .push(EndpointArg::Value(ValueArg::Single(Felt::from(n_used_instances).into())));
    Ok(())
}

/// Push the program's output into the implicit return values.
fn push_program_output(
    output_builtin_runner: &BuiltinRunner,
    implicit_return_values: &mut Vec<EndpointArg>,
    vm: &VirtualMachine,
) -> Result<(), LoadReturnValueError> {
    let output_builtin_segment = output_builtin_runner.base();
    let ptr_to_segment =
        Relocatable { segment_index: isize::try_from(output_builtin_segment).unwrap(), offset: 0 };
    let output =
        vm.get_integer_range(ptr_to_segment, output_builtin_runner.get_used_cells(&vm.segments)?)?;
    let output: Vec<MaybeRelocatable> =
        output.into_iter().map(|cow| cow.into_owned().into()).collect();
    implicit_return_values.push(EndpointArg::Value(ValueArg::Array(output)));
    Ok(())
}

/// Loads the explicit and implicit return values from the VM.
/// The implicit & explicit return values params are used to determine the return
/// values' structures (their values are ignored).
/// If the endpoint used builtins, the respective returned (implicit) arg is the builtin instance
/// usage, unless the builtin is the output builtin, in which case the arg is the output.
fn get_return_values(
    implicit_return_values_structures: &[ImplicitArg],
    explicit_return_values_structures: &[EndpointArg],
    vm: &VirtualMachine,
) -> Result<(Vec<EndpointArg>, Vec<EndpointArg>), LoadReturnValueError> {
    let return_args_len = implicit_return_values_structures
        .iter()
        .map(ImplicitArg::memory_length)
        .sum::<usize>()
        + explicit_return_values_structures.iter().map(EndpointArg::memory_length).sum::<usize>();
    let return_values_address = (vm.get_ap() - return_args_len)?;
    let mut current_address = return_values_address;

    let mut implicit_return_values: Vec<EndpointArg> = vec![];
    let mut builtin_runner_iterator = vm.get_builtin_runners().iter();
    for (i, implicit_arg) in implicit_return_values_structures.iter().enumerate() {
        debug!("Loading implicit return value {i}. Value: {implicit_arg:?}");
        match implicit_arg {
            ImplicitArg::Builtin(builtin) => {
                let curr_builtin_runner = builtin_runner_iterator
                    .next()
                    .unwrap_or_else(|| panic!("No builtin runner found for builtin {builtin}."));
                // Sanity check.
                let builtin_runner_segment_index =
                    isize::try_from(curr_builtin_runner.base()).unwrap();
                let return_value_segment_index = vm.get_relocatable(current_address)?.segment_index;
                assert_eq!(
                    builtin_runner_segment_index, return_value_segment_index,
                    "Builtin runner segment index {builtin_runner_segment_index} doesn't match \
                     return value's segment index {return_value_segment_index}."
                );

                match builtin {
                    BuiltinName::output => {
                        push_program_output(curr_builtin_runner, &mut implicit_return_values, vm)?
                    }
                    _ => {
                        push_n_used_instances(curr_builtin_runner, &mut implicit_return_values, vm)?
                    }
                }
                current_address = (current_address + implicit_arg.memory_length())?;
            }
            ImplicitArg::NonBuiltin(non_builtin_return_value) => {
                let (value, next_arg_address) =
                    load_endpoint_arg_from_address(non_builtin_return_value, current_address, vm)?;
                implicit_return_values.push(value);
                current_address = next_arg_address;
            }
        }
    }
    info!("Successfully loaded implicit return values.");

    let mut explicit_return_values: Vec<EndpointArg> = vec![];
    for (i, expected_return_value) in explicit_return_values_structures.iter().enumerate() {
        debug!("Loading explicit return value {i}. Value: {expected_return_value:?}");
        let (value, next_arg_address) =
            load_endpoint_arg_from_address(expected_return_value, current_address, vm)?;
        explicit_return_values.push(value);
        current_address = next_arg_address;
    }
    info!("Successfully loaded explicit return values.");

    Ok((implicit_return_values, explicit_return_values))
}

/// Runs a Cairo program's entrypoint and returns the explicit & implicit return values. It also
/// returns the cairo runner, to allow tests to perform additional validations.
/// Hint locals are added to the outermost exec scope.
/// If the endpoint used builtins, the respective returned (implicit) arg is the builtin instance
/// usage, unless the builtin is the output builtin, in which case the arg is the output.
#[allow(clippy::too_many_arguments)]
pub fn initialize_and_run_cairo_0_entry_point(
    runner_config: &EntryPointRunnerConfig,
    program_bytes: &[u8],
    entrypoint: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_return_values: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
    state_reader: Option<DictStateReader>,
) -> Cairo0EntryPointRunnerResult<(Vec<EndpointArg>, Vec<EndpointArg>, CairoRunner)> {
    // This function is split into to sub-functions to allow advanced use cases,
    // which require access to the cairo runner before running the entrypoint.
    let (mut cairo_runner, program, entrypoint) = initialize_cairo_runner(
        runner_config,
        program_bytes,
        entrypoint,
        implicit_args,
        hint_locals,
    )?;
    /// Skip parameter validations until they are fixed.
    let skip_parameter_validations = false;
    let (explicit_return_values, implicit_return_values) = run_cairo_0_entrypoint(
        entrypoint,
        explicit_args,
        implicit_args,
        state_reader,
        &mut cairo_runner,
        &program,
        runner_config,
        expected_explicit_return_values,
        skip_parameter_validations,
    )?;
    Ok((explicit_return_values, implicit_return_values, cairo_runner))
}

pub fn initialize_cairo_runner(
    runner_config: &EntryPointRunnerConfig,
    program_bytes: &[u8],
    entrypoint: &str,
    implicit_args: &[ImplicitArg], // Used to infer the builtins the program uses.
    hint_locals: HashMap<String, Box<dyn Any>>,
) -> Cairo0EntryPointRunnerResult<(CairoRunner, Program, String)> {
    let mut entrypoint = entrypoint.to_string();
    if runner_config.add_main_prefix_to_entrypoint {
        info!("Adding __main__ prefix to entrypoint.");
        entrypoint = format!("__main__.{entrypoint}");
    }

    let program = inject_builtins(program_bytes, implicit_args)?;
    info!("Successfully injected builtins into program.");

    let dynamic_layout_params = None;
    let disable_trace_padding = false;
    let mut cairo_runner = CairoRunner::new(
        &program,
        runner_config.layout,
        dynamic_layout_params,
        runner_config.proof_mode,
        runner_config.trace_enabled,
        disable_trace_padding,
    )
    .unwrap();
    for (key, value) in hint_locals.into_iter() {
        cairo_runner.exec_scopes.insert_box(&key, value);
    }
    let allow_missing_builtins = false;
    cairo_runner.initialize_builtins(allow_missing_builtins).unwrap();
    let program_base: Option<Relocatable> = None;
    cairo_runner.initialize_segments(program_base);
    info!("Created and initialized Cairo runner.");
    Ok((cairo_runner, program, entrypoint))
}

#[allow(clippy::too_many_arguments)]
pub fn run_cairo_0_entrypoint(
    entrypoint: String,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    state_reader: Option<DictStateReader>,
    cairo_runner: &mut CairoRunner,
    program: &Program,
    runner_config: &EntryPointRunnerConfig,
    expected_explicit_return_values: &[EndpointArg],
    // TODO(Aviv): Remove skip_parameter_validations once it is fixed.
    skip_parameter_validations: bool,
) -> Cairo0EntryPointRunnerResult<(Vec<EndpointArg>, Vec<EndpointArg>)> {
    // TODO(Amos): Perform complete validations.
    if skip_parameter_validations {
        info!("Basic Validations on explicit & implicit were skipped.");
    } else {
        perform_basic_validations_on_explicit_args(explicit_args, program, &entrypoint)?;
        perform_basic_validations_on_implicit_args(implicit_args, program, &entrypoint)?;
        info!("Performed basic validations on explicit & implicit args.");
    }

    let explicit_cairo_args: Vec<CairoArg> =
        explicit_args.iter().flat_map(EndpointArg::to_cairo_arg_vec).collect();
    let implicit_cairo_args = convert_implicit_args_to_cairo_args(implicit_args, &cairo_runner.vm);
    let entrypoint_args: Vec<&CairoArg> =
        implicit_cairo_args.iter().chain(explicit_cairo_args.iter()).collect();
    info!("Converted explicit & implicit args to Cairo args.");

    let (os_hints_config, os_state_input) = (None, None);
    let os_block_input = OsBlockInput::default();
    let mut hint_processor = SnosHintProcessor::new_for_testing(
        state_reader,
        program,
        os_hints_config,
        &os_block_input,
        os_state_input,
    )
    .unwrap_or_else(|err| panic!("Failed to create SnosHintProcessor: {err:?}"));
    info!("Program and Hint processor created successfully.");
    let program_segment_size: Option<usize> = None;
    cairo_runner
        .run_from_entrypoint(
            program
                .get_identifier(&entrypoint)
                .unwrap_or_else(|| panic!("entrypoint {entrypoint} not found."))
                .pc
                .unwrap(),
            &entrypoint_args,
            runner_config.verify_secure,
            program_segment_size,
            &mut hint_processor,
        )
        .map_err(Box::new)?;
    let execution_resources_after = cairo_runner.get_execution_resources().unwrap();
    info!(
        "execution resources after running entrypoint {entrypoint}: is \
         {execution_resources_after:?}"
    );

    info!("Successfully finished running entrypoint {entrypoint}");
    let (implicit_return_values, explicit_return_values) =
        get_return_values(implicit_args, expected_explicit_return_values, &cairo_runner.vm)?;
    Ok((implicit_return_values, explicit_return_values))
}
