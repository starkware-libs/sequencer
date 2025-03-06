use std::collections::HashMap;

use blockifier::execution::call_info::Retdata;
use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};
use cairo_vm::Felt252;
use serde_json::Value;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::test_utils::errors::Cairo0EntryPointRunnerError;

#[derive(Debug)]
pub enum ImplicitArg {
    Builtin(BuiltinName),
    Pointer(Relocatable),
    Felt(Felt252),
}

// Performs basic validations on the explicit arguments. A successful result from this function
// does NOT guarantee that the arguments are valid, because only basic types are checked.
fn validate_explicit_args(
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
        return Err(Cairo0EntryPointRunnerError::WrongNumberOfExplicitArgs {
            expected: expected_explicit_args.len(),
            actual: explicit_args.len(),
        });
    }

    expected_explicit_args.sort_by(|a, b| a.offset.cmp(&b.offset));
    for (index, actual_arg) in explicit_args.iter().enumerate() {
        let expected_arg = expected_explicit_args.get(index).unwrap();
        let actual_arg_is_felt = match actual_arg {
            CairoArg::Single(maybe_relocatable) => maybe_relocatable.get_int().is_some(),
            _ => false,
        };
        if expected_arg.cairo_type == "felt" && !actual_arg_is_felt {
            return Err(Cairo0EntryPointRunnerError::InvalidExplicitArg {
                error: format!(
                    "Mismatch for explicit argument {}. Expected arg is a {}, while actual arg is \
                     {:?}",
                    index, "felt", actual_arg
                ),
            });
        }
        if expected_arg.cairo_type == "felt*" && actual_arg_is_felt {
            return Err(Cairo0EntryPointRunnerError::InvalidExplicitArg {
                error: format!(
                    "Mismatch for explicit argument {}. Expected arg is a {}, while actual arg is \
                     {:?}",
                    index, "felt*", actual_arg
                ),
            });
        }
    }
    Ok(())
}

// Performs basic validations on the implicit arguments. A successful result from this function
// does NOT guarantee that the arguments are valid, because only basic types are checked.
fn validate_implicit_args(
    implicit_args: &[ImplicitArg],
    program: &Program,
    entrypoint: &str,
) -> Result<(), Cairo0EntryPointRunnerError> {
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
    if expected_implicit_args.len() != implicit_args.len() {
        return Err(Cairo0EntryPointRunnerError::WrongNumberOfImplicitArgs {
            expected: expected_implicit_args.len(),
            actual: implicit_args.len(),
        });
    }

    expected_implicit_args.sort_by(|a, b| a.1.offset.cmp(&b.1.offset));
    for (index, actual_arg) in implicit_args.iter().enumerate() {
        let (expected_arg_name, expected_arg) = expected_implicit_args.get(index).unwrap();
        if let Some(name_without_suffix) = expected_arg_name.strip_suffix("_ptr") {
            if let Some(builtin) = BuiltinName::from_str(name_without_suffix) {
                if let ImplicitArg::Builtin(actual_builtin) = actual_arg {
                    if *actual_builtin != builtin {
                        return Err(Cairo0EntryPointRunnerError::InvalidImplicitArg {
                            error: format!(
                                "Expected implicit argument {:?} to be builtin {:?}, got {:?}.",
                                index, builtin, actual_builtin
                            ),
                        });
                    }
                } else {
                    return Err(Cairo0EntryPointRunnerError::InvalidImplicitArg {
                        error: format!(
                            "Expected implicit argument {} to be builtin {:?}, got {:?}.",
                            index, builtin, actual_arg
                        ),
                    });
                }
                continue;
            }
        }
        if let ImplicitArg::Builtin(builtin) = actual_arg {
            return Err(Cairo0EntryPointRunnerError::InvalidImplicitArg {
                error: format!(
                    "Implicit argument {} should not be a builtin. expected argument is {:?}, \
                     actual arg is {:?}",
                    index, expected_arg, builtin
                ),
            });
        }
        if expected_arg.cairo_type == "felt" {
            match actual_arg {
                ImplicitArg::Felt(_) => continue,
                _ => {
                    return Err(Cairo0EntryPointRunnerError::InvalidImplicitArg {
                        error: format!(
                            "Implicit argument {} is expected to be a felt. got {:?} instead",
                            index, actual_arg
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

pub fn run_cairo_0_entry_point(
    program_str: &str,
    entrypoint: &str,
    n_expected_return_values: usize,
    explicit_args: &[CairoArg],
    implicit_args: &[ImplicitArg],
) -> Result<Retdata, Cairo0EntryPointRunnerError> {
    // This is a hack to add the entrypoint's builtins:
    // Create a program with all the builtins, and only use the relevant builtins for the
    // entrypoint.
    // TODO(Amos): Add builtins properly once the VM allows loading an entrypoint's builtins.
    // In addition, pass program as struct and add hint porcessor as param.
    let mut program_dict: HashMap<String, Value> = serde_json::from_str(program_str)?;
    let all_builtins = [
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
    program_dict
        .insert("builtins".to_string(), Value::from_iter(all_builtins.iter().map(|b| b.to_str())));
    let program_str_with_builtins = serde_json::to_string(&program_dict)?;
    let program = Program::from_bytes(program_str_with_builtins.as_bytes(), None)?;

    let mut hint_processor = SnosHintProcessor::new_for_testing(None, None, Some(program.clone()));

    validate_explicit_args(explicit_args, &program, entrypoint)?;
    validate_implicit_args(implicit_args, &program, entrypoint)?;

    let proof_mode = false;
    let trace_enabled = true;
    let mut cairo_runner =
        CairoRunner::new(&program, LayoutName::all_cairo, proof_mode, trace_enabled).unwrap();

    let allow_missing_builtins = false;
    cairo_runner.initialize_builtins(allow_missing_builtins).unwrap();
    let program_base: Option<Relocatable> = None;
    cairo_runner.initialize_segments(program_base);

    let all_builtins_initial_stacks: Vec<Vec<MaybeRelocatable>> = cairo_runner
        .vm
        .get_builtin_runners()
        .iter()
        .map(|builtin_runner| builtin_runner.initial_stack())
        .collect();
    let all_builtin_map: HashMap<_, _> =
        all_builtins.iter().zip(all_builtins_initial_stacks).collect();
    let used_builtins: Vec<&BuiltinName> = implicit_args
        .iter()
        .filter_map(|arg| match arg {
            ImplicitArg::Builtin(builtin) => Some(builtin),
            _ => None,
        })
        .collect();
    let mut entrypoint_args: Vec<CairoArg> =
        used_builtins.iter().flat_map(|b| all_builtin_map[b].clone()).map(CairoArg::from).collect();
    entrypoint_args.extend_from_slice(explicit_args);
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

    // TODO(Amos): Return non-builtin implicit arguments.
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
